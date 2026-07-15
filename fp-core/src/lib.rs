//! Platform-independent core of the Faderpunk firmware: the app API, all apps,
//! layout management, storage layout, and the portable halves of the hardware
//! tasks (types, statics and protocol/engine logic). The `faderpunk` firmware
//! crate and the desktop simulator both build on this crate and provide the
//! actual hardware (or virtual hardware) behind the shared statics.
#![no_std]

// Order matters: `fmt` and `macros` define macros used by the modules below.
#[macro_use]
mod fmt;
#[macro_use]
mod macros;

pub mod app;
pub mod apps;
pub mod events;
pub mod layout;
pub mod platform;
pub mod state;
pub mod storage;
pub mod tasks;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::lazy_lock::LazyLock;
use embassy_sync::mutex::Mutex;
use libfp::quantizer::Quantizer;

/// Raw mutex for channels that are only ever touched from a single executor.
/// On the RP2350 this is `ThreadModeRawMutex` (free); on other targets (host
/// simulator) it falls back to a critical-section mutex.
#[cfg(all(target_arch = "arm", target_os = "none"))]
pub type CoreLocalRawMutex = embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
pub type CoreLocalRawMutex = CriticalSectionRawMutex;

pub static QUANTIZER: LazyLock<Mutex<CriticalSectionRawMutex, Quantizer>> =
    LazyLock::new(|| Mutex::new(Quantizer::default()));

// Root re-exports used by the `register_apps!` macro expansion.
pub use tasks::i2c::I2C_LEADER_CHANNEL;
pub use tasks::max::MAX_CHANNEL;
pub use tasks::midi::APP_MIDI_CHANNEL;
