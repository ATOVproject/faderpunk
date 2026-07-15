//! Portable MIDI plumbing: message types, the app→distributor→output channel
//! chain, NRPN assembly and the shared RX event processing. The physical
//! transports (USB, UART DIN) live in the firmware; the simulator bridges the
//! same channels to virtual MIDI ports.

use embassy_futures::select::{select, Either};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Sender},
    pubsub::{PubSubChannel, Publisher, Subscriber},
};
use embassy_time::{Duration, Instant, Ticker};
use heapless::{Deque, Vec};
use midly::{
    live::{LiveEvent, SystemCommon, SystemRealtime},
    num::{u4, u7},
    MidiMessage,
};

use libfp::{ClockSrc, MidiOut, GLOBAL_CHANNELS};

use crate::events::{EventPubSubPublisher, InputEvent};
use crate::tasks::clock::{ClockInEvent, SyncEngineEvent};
use crate::tasks::configure::CONFIG_FRAME_BUF;
use crate::CoreLocalRawMutex;

const MIDI_CHANNEL_SIZE: usize = 16;
const MIDI_APP_QUEUE_SIZE: usize = 16;
const MIDI_PUBSUB_SIZE: usize = 64;
const MIDI_BURST_PER_TICK: usize = 8;
// Max apps
const MIDI_PUBSUB_SUBS: usize = GLOBAL_CHANNELS;
// Only one, from here
const MIDI_PUBSUB_SENDERS: usize = 1;

#[derive(Clone, Copy)]
pub enum MidiEventSource {
    Local,
    Passthrough,
}

#[derive(Clone, Copy)]
pub enum MidiMsg {
    Live {
        event: LiveEvent<'static>,
        target: MidiOut,
        source: MidiEventSource,
    },
    Nrpn {
        channel: u4,
        param: u16,
        value: u16,
        target: MidiOut,
    },
}

impl MidiMsg {
    pub fn new(event: LiveEvent<'static>, target: MidiOut, source: MidiEventSource) -> Self {
        Self::Live {
            event,
            target,
            source,
        }
    }

    pub fn nrpn(channel: u4, param: u16, value: u16, target: MidiOut) -> Self {
        Self::Nrpn {
            channel,
            param,
            value,
            target,
        }
    }
}

#[derive(Clone, Copy)]
pub struct MidiClockMsg {
    pub event: SystemRealtime,
    pub target: MidiOut,
}

impl MidiClockMsg {
    pub fn new(event: SystemRealtime, target: MidiOut) -> Self {
        Self { event, target }
    }
}

#[derive(Clone, Copy)]
pub enum MidiOutEvent {
    Event(MidiMsg),
    Clock(MidiClockMsg),
}

#[derive(Clone, Copy)]
pub enum MidiEvent {
    Live(LiveEvent<'static>),
    Nrpn { channel: u4, param: u16, value: u16 },
}

pub static MIDI_CHANNEL: Channel<CriticalSectionRawMutex, MidiOutEvent, MIDI_CHANNEL_SIZE> =
    Channel::new();

// Channel for apps (Core 1) to send MIDI to the distributor task (Core 1)
pub static APP_MIDI_CHANNEL: Channel<CoreLocalRawMutex, (usize, MidiMsg), MIDI_CHANNEL_SIZE> =
    Channel::new();

pub type AppMidiSender = Sender<'static, CoreLocalRawMutex, (usize, MidiMsg), MIDI_CHANNEL_SIZE>;

// Define the type once
pub type MidiPubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubPublisher = Publisher<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

// Instantiate specific channels for your sources
pub static MIDI_USB_PUBSUB: MidiPubSubChannel = PubSubChannel::new();
pub static MIDI_DIN_PUBSUB: MidiPubSubChannel = PubSubChannel::new();

#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum CodeIndexNumber {
    /// Miscellaneous function codes. Reserved for future extensions.
    MiscFunction = 0x0,
    /// Cable events. Reserved for future expansion.
    CableEvents = 0x1,
    /// Two-byte System Common messages like MTC, SongSelect, etc.
    SystemCommonLen2 = 0x2,
    /// Three-byte System Common messages like SPP, etc.
    SystemCommonLen3 = 0x3,
    /// SysEx starts or continues.
    SysExStarts = 0x4,
    /// Single-byte System Common Message or SysEx ends with following single byte.
    SystemCommonLen1 = 0x5,
    /// SysEx ends with following two bytes.
    SysExEndsNext2 = 0x6,
    /// SysEx ends with following three bytes.
    SysExEndsNext3 = 0x7,
    /// Note Off
    NoteOff = 0x8,
    /// Note On
    NoteOn = 0x9,
    /// Polyphonic Key Pressure (Aftertouch)
    KeyPressure = 0xA,
    /// Control Change
    ControlChange = 0xB,
    /// Program Change
    ProgramChange = 0xC,
    /// Channel Pressure (Aftertouch)
    ChannelPressure = 0xD,
    /// Pitch Bend Change
    PitchBendChange = 0xE,
    /// Single-byte
    SingleByte = 0xF,
}

#[embassy_executor::task]
pub async fn midi_distributor() {
    let mut app_queues: [Deque<MidiMsg, MIDI_APP_QUEUE_SIZE>; 16] =
        core::array::from_fn(|_| Deque::new());
    let mut last_app_id: usize = 0;
    let midi_out_sender = MIDI_CHANNEL.sender();
    let app_midi_receiver = APP_MIDI_CHANNEL.receiver();
    let mut ticker = Ticker::every(Duration::from_millis(2));

    loop {
        match select(app_midi_receiver.receive(), ticker.next()).await {
            // A new message from an app has arrived, enqueue it.
            Either::First((start_channel, ev)) => {
                if !app_queues[start_channel].is_full() {
                    let _ = app_queues[start_channel].push_back(ev);
                }
            }
            // The throttle timer has fired, send a small burst.
            Either::Second(_) => {
                for _ in 0..MIDI_BURST_PER_TICK {
                    let mut sent = false;

                    // Find the next app with a message in its queue (round-robin)
                    for i in 0..16 {
                        let app_idx = (last_app_id + 1 + i) % 16;
                        if let Some(ev) = app_queues[app_idx].pop_front() {
                            midi_out_sender.send(MidiOutEvent::Event(ev)).await;
                            last_app_id = app_idx;
                            sent = true;
                            break;
                        }
                    }

                    if !sent {
                        break;
                    }
                }
            }
        }
    }
}

/// Reassembles SysEx frames from cable-1 USB-MIDI event packets (CIN 0x4
/// start/continue, 0x5/0x6/0x7 end). Collects the frame body without the
/// F0/F7 delimiters. Oversized frames are dropped whole.
pub struct SysExAssembler {
    buf: Vec<u8, CONFIG_FRAME_BUF>,
    active: bool,
    overflow: bool,
}

impl Default for SysExAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl SysExAssembler {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            active: false,
            overflow: false,
        }
    }

    /// Feed the data bytes of one event packet. Returns true when a complete
    /// frame is available via [`Self::frame`]; call [`Self::clear`] after
    /// consuming it.
    pub fn feed(&mut self, cin: u8, data: &[u8]) -> bool {
        let mut bytes = data;
        if !self.active {
            // A frame must open with SysEx start
            if bytes.first() != Some(&0xF0) {
                return false;
            }
            self.buf.clear();
            self.overflow = false;
            self.active = true;
            bytes = &bytes[1..];
        }
        // End CINs carry a trailing F7 that is not part of the body
        let is_end = (0x5..=0x7).contains(&cin);
        if is_end && bytes.last() == Some(&0xF7) {
            bytes = &bytes[..bytes.len() - 1];
        }
        if self.buf.extend_from_slice(bytes).is_err() {
            self.overflow = true;
        }
        if !is_end {
            return false;
        }
        self.active = false;
        if self.overflow {
            warn!("Config SysEx frame overflow, dropping");
            self.buf.clear();
            self.overflow = false;
            return false;
        }
        true
    }

    pub fn frame(&self) -> &[u8] {
        &self.buf
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

#[derive(Default)]
pub struct NrpnTracker {
    param_msb: Option<u8>,
    param_lsb: Option<u8>,
    value_msb: Option<u8>,
}

impl NrpnTracker {
    /// Process a CC message. Returns Some(MidiEvent) if a complete NRPN message was assembled
    /// or if a non-NRPN CC should be forwarded. Returns None if the CC was consumed as part of
    /// an NRPN sequence.
    fn process_cc(&mut self, channel: u4, controller: u7, value: u7) -> Option<MidiEvent> {
        let cc = controller.as_int();
        match cc {
            99 => {
                self.param_msb = Some(value.as_int());
                self.value_msb = None;
                None
            }
            98 => {
                self.param_lsb = Some(value.as_int());
                self.value_msb = None;
                None
            }
            6 => {
                if self.param_msb.is_some() && self.param_lsb.is_some() {
                    self.value_msb = Some(value.as_int());
                    None
                } else {
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            38 => {
                if let Some(val_msb) = self.value_msb.take() {
                    let param = ((self.param_msb.unwrap_or(0) as u16) << 7)
                        | (self.param_lsb.unwrap_or(0) as u16);
                    let nrpn_value = ((val_msb as u16) << 7) | (value.as_int() as u16);
                    Some(MidiEvent::Nrpn {
                        channel,
                        param,
                        value: nrpn_value,
                    })
                } else {
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            _ => {
                // Non-NRPN CC — pass through
                Some(MidiEvent::Live(LiveEvent::Midi {
                    channel,
                    message: MidiMessage::Controller { controller, value },
                }))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process_midi_event(
    event: &LiveEvent<'_>,
    publisher: &MidiPubSubPublisher,
    nrpn_trackers: &mut [NrpnTracker; 16],
    thru_targets: [bool; 3],
    clock_src: ClockSrc,
    sync_engine_sender: &Sender<'static, CoreLocalRawMutex, SyncEngineEvent, 16>,
    midi_sender: &Sender<'static, CriticalSectionRawMutex, MidiOutEvent, 16>,
    event_publisher: &EventPubSubPublisher,
) {
    match event {
        LiveEvent::Realtime(msg) => match msg {
            SystemRealtime::TimingClock => {
                sync_engine_sender
                    .send(SyncEngineEvent::Pulse {
                        source: clock_src,
                        timestamp: Instant::now(),
                    })
                    .await;
            }
            SystemRealtime::Start => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Start(clock_src)))
                    .await;
            }
            SystemRealtime::Stop => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Stop(clock_src)))
                    .await;
            }
            SystemRealtime::Continue => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Continue(
                        clock_src,
                    )))
                    .await;
            }
            SystemRealtime::Reset => {
                sync_engine_sender
                    .send(SyncEngineEvent::Transport(ClockInEvent::Reset(clock_src)))
                    .await;
            }
            _ => {}
        },
        LiveEvent::Midi { channel, message } => {
            // Check for program change 0-15 and trigger scene load
            if let MidiMessage::ProgramChange { program } = message {
                let program_num = program.as_int();
                if program_num <= 15 {
                    event_publisher.publish_immediate(InputEvent::LoadSceneFromMidi(program_num));
                }
            }

            let ev = event.to_static();
            // Always pass raw event through for MIDI thru
            midi_sender
                .send(MidiOutEvent::Event(MidiMsg::new(
                    ev,
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                )))
                .await;

            // Route CC through NRPN tracker
            if let MidiMessage::Controller { controller, value } = message {
                let tracker = &mut nrpn_trackers[channel.as_int() as usize];
                if let Some(midi_event) = tracker.process_cc(*channel, *controller, *value) {
                    publisher.publish_immediate(midi_event);
                }
            } else {
                publisher.publish_immediate(MidiEvent::Live(ev));
            }
        }
        _ => {
            let ev = event.to_static();
            publisher.publish_immediate(MidiEvent::Live(ev));
            midi_sender
                .send(MidiOutEvent::Event(MidiMsg::new(
                    ev,
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                )))
                .await;
        }
    }
}

pub fn cin_from_live_event(midi_ev: &LiveEvent) -> CodeIndexNumber {
    match midi_ev {
        LiveEvent::Realtime(..) => CodeIndexNumber::SingleByte,
        LiveEvent::Midi { message, .. } => match message {
            MidiMessage::NoteOn { .. } => CodeIndexNumber::NoteOn,
            MidiMessage::NoteOff { .. } => CodeIndexNumber::NoteOff,
            MidiMessage::Aftertouch { .. } => CodeIndexNumber::KeyPressure,
            MidiMessage::ChannelAftertouch { .. } => CodeIndexNumber::ChannelPressure,
            MidiMessage::ProgramChange { .. } => CodeIndexNumber::ProgramChange,
            MidiMessage::Controller { .. } => CodeIndexNumber::ControlChange,
            MidiMessage::PitchBend { .. } => CodeIndexNumber::PitchBendChange,
        },
        LiveEvent::Common(common_message) => match common_message {
            SystemCommon::SysEx(data) => {
                // TODO: Implement stateful SysEx CIN determination once needed
                if data.is_empty() {
                    CodeIndexNumber::SysExEndsNext3
                } else {
                    CodeIndexNumber::SysExStarts
                }
            }
            SystemCommon::SongSelect(..) => CodeIndexNumber::SystemCommonLen2,
            SystemCommon::TuneRequest => CodeIndexNumber::SingleByte,
            SystemCommon::Undefined(..) => CodeIndexNumber::MiscFunction,
            SystemCommon::SongPosition(..) => CodeIndexNumber::SystemCommonLen3,
            SystemCommon::MidiTimeCodeQuarterFrame(..) => CodeIndexNumber::SystemCommonLen2,
        },
    }
}

pub fn len_from_cin(cin: u8) -> usize {
    match cin & 0x0f {
        0x5 | 0xf => 1,
        0x2 | 0x6 | 0xc | 0xd => 2,
        0x3 | 0x4 | 0x7 | 0x8 | 0x9 | 0xa | 0xb | 0xe => 3,
        _ => 0,
    }
}
