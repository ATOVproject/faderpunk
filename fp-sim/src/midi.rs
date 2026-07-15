//! Virtual MIDI ports: the simulator appears as a "Faderpunk Sim" MIDI device
//! (CoreMIDI on macOS, ALSA on Linux). Performance MIDI and the configurator
//! SysEx protocol share the single port pair — config frames are recognized by
//! their `F0 7D 46 50 01` header, everything else is treated as performance
//! MIDI, mirroring what the USB cables do on hardware.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{Ignore, MidiInput, MidiOutput, MidiOutputConnection};
use midly::live::LiveEvent;
use midly::num::u7;
use midly::MidiMessage;
use static_cell::StaticCell;

use libfp::sysex::SYSEX_HEADER;
use libfp::utils::scale_bits_12_14;
use libfp::{ClockSrc, MidiIn, MidiOutConfig, MidiOutMode};

use fp_core::events::EVENT_PUBSUB;
use fp_core::tasks::clock::SYNC_ENGINE_CHANNEL;
use fp_core::tasks::configure::{ConfigSink, ProtocolError, CONFIG_RX_CHANNEL};
use fp_core::tasks::global_config::get_global_config;
use fp_core::tasks::midi::{
    process_midi_event, MidiEventSource, MidiMsg, MidiOutEvent, NrpnTracker, MIDI_CHANNEL,
    MIDI_USB_PUBSUB,
};

use crate::FIRMWARE_VERSION;

const CLIENT_NAME: &str = "Faderpunk Sim";

/// Raw inbound MIDI chunks from the midir callback thread.
const RX_CHUNK: usize = 1024;
static MIDI_RX: Channel<CriticalSectionRawMutex, heapless::Vec<u8, RX_CHUNK>, 16> = Channel::new();

pub type SharedMidiOut = Mutex<CriticalSectionRawMutex, MidiOutputConnection>;

static MIDI_OUT: StaticCell<SharedMidiOut> = StaticCell::new();

/// Creates the virtual input/output port pair. The input connection is leaked
/// so its callback stays alive for the lifetime of the process.
pub fn create_virtual_ports() -> &'static SharedMidiOut {
    let mut input = MidiInput::new(CLIENT_NAME).expect("failed to create MIDI input client");
    input.ignore(Ignore::None);
    let conn_in = input
        .create_virtual(
            CLIENT_NAME,
            |_timestamp, bytes, _| {
                let mut chunk = heapless::Vec::new();
                if chunk.extend_from_slice(bytes).is_err() {
                    log::warn!("Inbound MIDI chunk larger than {RX_CHUNK} bytes, dropping");
                    return;
                }
                if MIDI_RX.try_send(chunk).is_err() {
                    log::warn!("MIDI RX queue full, dropping chunk");
                }
            },
            (),
        )
        .expect("failed to create virtual MIDI input port");
    std::mem::forget(conn_in);

    let output = MidiOutput::new(CLIENT_NAME).expect("failed to create MIDI output client");
    let conn_out = output
        .create_virtual(CLIENT_NAME)
        .expect("failed to create virtual MIDI output port");

    log::info!("Virtual MIDI ports created: \"{CLIENT_NAME}\"");
    MIDI_OUT.init(Mutex::new(conn_out))
}

async fn send_bytes(out: &'static SharedMidiOut, bytes: &[u8]) {
    let mut conn = out.lock().await;
    if let Err(err) = conn.send(bytes) {
        log::warn!("Failed to send MIDI: {err}");
    }
}

async fn send_event(out: &'static SharedMidiOut, event: LiveEvent<'_>) {
    let mut buf = [0u8; 3];
    let mut cursor = midly::io::Cursor::new(&mut buf);
    event.write(&mut cursor).unwrap();
    let len = cursor.cursor();
    send_bytes(out, &buf[..len]).await;
}

/// Drains the app→output MIDI channel to the virtual port. Only the USB
/// target (index 0) maps to the simulator port; the DIN targets have no
/// physical counterpart here.
#[embassy_executor::task]
pub async fn midi_out_bridge(out: &'static SharedMidiOut) {
    let midi_receiver = MIDI_CHANNEL.receiver();

    loop {
        match midi_receiver.receive().await {
            MidiOutEvent::Event(MidiMsg::Live {
                event,
                target,
                source,
            }) => {
                let enabled = match source {
                    // Match the firmware: local events are dropped on outs
                    // configured as strict THRU or disabled.
                    MidiEventSource::Local => {
                        let disabled = matches!(
                            get_global_config().midi.outs[0],
                            MidiOutConfig {
                                mode: MidiOutMode::MidiThru { .. },
                                ..
                            } | MidiOutConfig {
                                mode: MidiOutMode::None,
                                ..
                            }
                        );
                        target.0[0] && !disabled
                    }
                    MidiEventSource::Passthrough => target.0[0],
                };
                if enabled {
                    send_event(out, event).await;
                }
            }
            MidiOutEvent::Event(MidiMsg::Nrpn {
                channel,
                param,
                value,
                target,
            }) => {
                if !target.0[0] {
                    continue;
                }
                let value_14 = scale_bits_12_14(value);
                let ccs: [(u8, u8); 4] = [
                    (99, (param >> 7) as u8),
                    (98, (param & 0x7F) as u8),
                    (6, (value_14 >> 7) as u8),
                    (38, (value_14 & 0x7F) as u8),
                ];
                for (controller, cc_value) in ccs {
                    let event = LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller {
                            controller: u7::new(controller),
                            value: u7::new(cc_value),
                        },
                    };
                    send_event(out, event).await;
                }
            }
            MidiOutEvent::Clock(msg) => {
                if msg.target.0[0] {
                    send_event(out, LiveEvent::Realtime(msg.event)).await;
                }
            }
        }
    }
}

/// Parses inbound MIDI from the virtual port and feeds it through the same
/// processing chain as the firmware's USB RX path. SysEx frames carrying the
/// config header go to the config loop instead.
#[embassy_executor::task]
pub async fn midi_in_bridge() {
    let sync_engine_sender = SYNC_ENGINE_CHANNEL.sender();
    let midi_sender = MIDI_CHANNEL.sender();
    let usb_publisher = MIDI_USB_PUBSUB.publisher().unwrap();
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();
    let mut nrpn_trackers: [NrpnTracker; 16] = Default::default();

    // SysEx frames can span multiple midir callbacks; accumulate F0..F7.
    let mut sysex: Vec<u8> = Vec::new();
    let mut in_sysex = false;

    loop {
        let chunk = MIDI_RX.receive().await;
        let mut bytes: &[u8] = &chunk;

        if in_sysex || bytes.first() == Some(&0xF0) {
            if !in_sysex {
                sysex.clear();
                in_sysex = true;
            }
            if let Some(end) = bytes.iter().position(|&b| b == 0xF7) {
                sysex.extend_from_slice(&bytes[..end]);
                in_sysex = false;
                handle_sysex(&sysex);
                bytes = &bytes[end + 1..];
            } else {
                sysex.extend_from_slice(bytes);
                continue;
            }
            if bytes.is_empty() {
                continue;
            }
        }

        match LiveEvent::parse(bytes) {
            Ok(event) => {
                // Match the firmware's USB-source passthrough routing
                let thru_targets = get_global_config().midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        }
                    )
                });
                process_midi_event(
                    &event,
                    &usb_publisher,
                    &mut nrpn_trackers,
                    thru_targets,
                    ClockSrc::MidiUsb,
                    &sync_engine_sender,
                    &midi_sender,
                    &event_publisher,
                )
                .await;
            }
            Err(err) => {
                log::debug!("Unparseable MIDI input ({} bytes): {err}", bytes.len());
            }
        }
    }
}

/// `frame` is the SysEx body starting with F0, without the trailing F7.
fn handle_sysex(frame: &[u8]) {
    let Some(body) = frame.strip_prefix(&[0xF0]) else {
        return;
    };
    if !body.starts_with(&SYSEX_HEADER) {
        log::debug!("Ignoring non-config SysEx ({} bytes)", frame.len());
        return;
    }
    match heapless::Vec::from_slice(body) {
        Ok(frame) => {
            if CONFIG_RX_CHANNEL.try_send(frame).is_err() {
                log::warn!("Config RX channel full, dropping frame");
            }
        }
        Err(()) => log::warn!("Config frame too large, dropping"),
    }
}

struct SimConfigSink {
    out: &'static SharedMidiOut,
}

impl ConfigSink for SimConfigSink {
    async fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProtocolError> {
        let mut conn = self.out.lock().await;
        conn.send(frame)
            .map_err(|_| ProtocolError::TransmissionError)
    }
}

#[embassy_executor::task]
pub async fn config_loop(out: &'static SharedMidiOut) {
    fp_core::tasks::configure::start_config_loop(SimConfigSink { out }, FIRMWARE_VERSION).await
}
