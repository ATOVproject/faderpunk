#![no_std]

#[macro_use]
mod macros;

pub mod app;
pub mod apps;
pub mod config;
pub mod tasks;
mod utils;

use config::GlobalConfig;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    pubsub::PubSubChannel,
    watch::Watch,
};
use midi2::channel_voice1::ChannelVoice1;
use portable_atomic::{AtomicBool, AtomicU16};
use static_cell::StaticCell;
use tasks::{leds::LedsAction, max::MaxConfig};

pub type XTxSender = Sender<'static, NoopRawMutex, (usize, XTxMsg), 128>;

/// Messages from core 0 to core 1
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum XTxMsg {
    ButtonDown,
    FaderChange,
    Clock,
}

/// Messages from core 1 to core 0
#[derive(Clone)]
pub enum XRxMsg {
    MaxPortReconfigure(MaxConfig),
    MidiMessage(ChannelVoice1<[u8; 3]>),
    SetLed(LedsAction),
}

pub static ATOMIC_VALUES: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];

pub static WATCH_SCENE_SET: Watch<CriticalSectionRawMutex, &[usize], 18> = Watch::new();
// FIXME: Use StaticCell or similar with NoopRawMutex
pub static CHANS_X: [PubSubChannel<CriticalSectionRawMutex, (usize, XTxMsg), 64, 5, 1>; 16] =
    [const { PubSubChannel::new() }; 16];
/// Collector channel on core 0
pub static CHAN_X_0: StaticCell<Channel<NoopRawMutex, (usize, XTxMsg), 128>> = StaticCell::new();
/// Collector channel on core 1
pub static CHAN_X_1: StaticCell<Channel<NoopRawMutex, (usize, XRxMsg), 128>> = StaticCell::new();
/// Channel from core 0 to core 1
pub static CHAN_X_TX: Channel<CriticalSectionRawMutex, (usize, XTxMsg), 64> = Channel::new();
/// Channel from core 1 to core 0
pub static CHAN_X_RX: Channel<CriticalSectionRawMutex, (usize, XRxMsg), 64> = Channel::new();
/// Channel for sending messages to the MAX
pub static CHAN_MAX: StaticCell<Channel<NoopRawMutex, (usize, MaxConfig), 64>> = StaticCell::new();
/// Channel for sending messages to the MIDI bus
pub static CHAN_MIDI: StaticCell<Channel<NoopRawMutex, (usize, ChannelVoice1<[u8; 3]>), 64>> =
    StaticCell::new();
/// Channel for sending messages to the LEDs
pub static CHAN_LEDS: StaticCell<Channel<NoopRawMutex, (usize, LedsAction), 64>> =
    StaticCell::new();
/// Channel for sending messages to the clock
pub static CHAN_CLOCK: StaticCell<Channel<NoopRawMutex, u16, 64>> = StaticCell::new();
/// Tasks (apps) that are currently running (number 17 is the publisher task)
pub static CORE1_TASKS: [AtomicBool; 17] = [const { AtomicBool::new(false) }; 17];
pub static GLOBAL_CONFIG: StaticCell<GlobalConfig> = StaticCell::new();
