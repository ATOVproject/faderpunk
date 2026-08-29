//! Post-mortem beacon for silent Core-1 deaths.
//!
//! Core 1 aborting leaves the device looking healthy: Core 0 keeps USB and the
//! MIDI clock alive, so the only symptom is that app notes stop. Without a
//! debug probe there is no way to see where it died.
//!
//! The panic handler records `file`/`line` here, and a Core-0 task re-sends
//! them as CCs on MIDI channel 16 once per second, so a plain MIDI capture
//! carries the panic site. `scripts/decode-panic-beacon.py` maps the file hash
//! back to a path.

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use libfp::MidiOut;
use midly::{live::LiveEvent, num::u4, num::u7, MidiMessage};
use portable_atomic::{AtomicBool, AtomicU32, Ordering};

use crate::tasks::midi::{MidiEventSource, MidiMsg, MidiOutEvent, MIDI_CHANNEL};

/// Channel 16 (0-based 15) — outside the range apps normally use.
const BEACON_CHANNEL: u8 = 15;

const CC_MARKER: u8 = 110;
const CC_LINE_LO: u8 = 111;
const CC_LINE_HI: u8 = 112;
const CC_FILE_0: u8 = 113;
const CC_FILE_1: u8 = 114;
const CC_FILE_2: u8 = 115;

static PANICKED: AtomicBool = AtomicBool::new(false);
static PANIC_LINE: AtomicU32 = AtomicU32::new(0);
static PANIC_FILE_HASH: AtomicU32 = AtomicU32::new(0);

/// FNV-1a, truncated to 21 bits so it fits three 7-bit CC values.
pub const fn file_hash(path: &str) -> u32 {
    let bytes = path.as_bytes();
    let mut hash: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u32;
        hash = hash.wrapping_mul(0x0100_0193);
        i += 1;
    }
    hash & 0x001f_ffff
}

/// Called from the panic handler on whichever core died.
pub fn record(file: &str, line: u32) {
    PANIC_FILE_HASH.store(file_hash(file), Ordering::Relaxed);
    PANIC_LINE.store(line, Ordering::Relaxed);
    PANICKED.store(true, Ordering::Relaxed);
}

fn cc(controller: u8, value: u8) -> LiveEvent<'static> {
    LiveEvent::Midi {
        channel: u4::new(BEACON_CHANNEL),
        message: MidiMessage::Controller {
            controller: u7::new(controller),
            value: u7::new(value & 0x7F),
        },
    }
}

pub async fn start_panic_beacon(spawner: &Spawner) {
    spawner.spawn(panic_beacon()).unwrap();
}

#[embassy_executor::task]
async fn panic_beacon() {
    loop {
        Timer::after(Duration::from_millis(1000)).await;
        if !PANICKED.load(Ordering::Relaxed) {
            continue;
        }

        let line = PANIC_LINE.load(Ordering::Relaxed);
        let hash = PANIC_FILE_HASH.load(Ordering::Relaxed);

        // Passthrough source: the Local mute and rate limiter must not be able
        // to swallow the one message that explains the failure.
        let frames = [
            cc(CC_MARKER, 127),
            cc(CC_LINE_LO, (line & 0x7F) as u8),
            cc(CC_LINE_HI, ((line >> 7) & 0x7F) as u8),
            cc(CC_FILE_0, (hash & 0x7F) as u8),
            cc(CC_FILE_1, ((hash >> 7) & 0x7F) as u8),
            cc(CC_FILE_2, ((hash >> 14) & 0x7F) as u8),
        ];
        for event in frames {
            MIDI_CHANNEL
                .send(MidiOutEvent::Event(MidiMsg::new(
                    event,
                    MidiOut([true, false, false]),
                    MidiEventSource::Passthrough,
                )))
                .await;
        }
    }
}
