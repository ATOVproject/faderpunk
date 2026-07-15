//! Platform hooks: the two things fp-core needs from its host that cannot be
//! expressed through the shared statics — a random source and a system reset.
//! The firmware installs RP2350 implementations (ROSC RNG, SCB reset); the
//! simulator installs host implementations. Must be called once at startup,
//! before any app or task runs.

use embassy_sync::once_lock::OnceLock;

pub struct Platform {
    /// Returns a uniformly random u16 (full range; callers reduce as needed).
    pub rand_u16: fn() -> u16,
    /// Performs a full system reset (or process exit on the simulator).
    pub sys_reset: fn() -> !,
}

static PLATFORM: OnceLock<Platform> = OnceLock::new();

/// Installs the platform hooks. Later calls are ignored.
pub fn init(platform: Platform) {
    let _ = PLATFORM.init(platform);
}

fn get() -> &'static Platform {
    PLATFORM
        .try_get()
        .expect("fp_core::platform::init must be called at startup")
}

pub(crate) fn rand_u16() -> u16 {
    (get().rand_u16)()
}

pub(crate) fn sys_reset() -> ! {
    (get().sys_reset)()
}
