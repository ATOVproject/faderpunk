//! Hardware watchdog, and the slot marker that survives a watchdog reset.
//!
//! Installable FPApps are native code we did not compile. Their `poll()` runs
//! synchronously on Core 1, so one that never returns takes the whole core with
//! it and no amount of async structure recovers from that. The watchdog is the
//! backstop: Core 1 feeds it while it is alive, and stops when it isn't.
//!
//! To turn "the device hung" into "this slot hung", `mark_slot` records the
//! slot being polled in `scratch0`, which survives the reset. Boot reads it to
//! quarantine the culprit rather than resetting forever.

use embassy_rp::pac;
use embassy_rp::watchdog::{ResetReason, Watchdog};
use embassy_time::Duration;
use portable_atomic::{AtomicU32, Ordering};

/// Steady-state period. Bounded above by the RP2350 ceiling of 16.777s, and
/// below by the longest legitimate stretch either core can spend unable to
/// feed. The longest such stretch is one 4 KiB flash sector erase (`erase()`
/// in `fpapps.rs` splits by sector precisely so this stays true), which is
/// hundreds of milliseconds at worst. The generous margin over that is
/// deliberate: a spurious reset on a healthy device is far worse than taking
/// a few extra seconds to recover a hung one.
pub const NORMAL_TIMEOUT: Duration = Duration::from_secs(8);

/// How often Core 1's heartbeat feeds. Comfortably inside `NORMAL_TIMEOUT` so
/// ordinary scheduling jitter can never starve it.
pub const FEED_INTERVAL: Duration = Duration::from_secs(1);

/// Zero until `arm()`, which makes `feed()` a no-op for the whole boot window
/// before the watchdog is running.
static LOAD_VALUE: AtomicU32 = AtomicU32::new(0);

/// High 28 bits tag the register as ours; the low nibble carries the slot.
/// `scratch0` powers up as zero, so an untagged register never decodes.
const SLOT_TAG_MAGIC: u32 = 0xFA51_0000;
const SLOT_TAG_MASK: u32 = 0xFFFF_FFF0;

/// Start the watchdog and enable `feed()`.
///
/// Deliberately called late in boot, once the slow one-time init is done: a
/// stall in firmware setup is not the hazard this guards against, and arming
/// early would only risk resetting a device mid-initialization.
pub fn arm(watchdog: &mut Watchdog) {
    watchdog.start(NORMAL_TIMEOUT);
    // Mirrors what `Watchdog::start` loaded, so `feed()` can reload the counter
    // from any core without owning the peripheral.
    LOAD_VALUE.store(NORMAL_TIMEOUT.as_micros() as u32, Ordering::Relaxed);
}

/// Reload the counter. Callable from either core: it is a single idempotent
/// register write, which is exactly what `Watchdog::feed` does, minus the
/// `&mut self` that would force this through a lock on a hot path.
pub fn feed() {
    let load_value = LOAD_VALUE.load(Ordering::Relaxed);
    if load_value != 0 {
        pac::WATCHDOG
            .load()
            .write_value(pac::watchdog::regs::Load(load_value));
    }
}

/// Record that `slot` is about to run untrusted code.
pub fn mark_slot(slot: u8) {
    pac::WATCHDOG
        .scratch0()
        .write_value(SLOT_TAG_MAGIC | u32::from(slot & 0xF));
}

/// Clear the marker. Must run on every path out of a poll, not just the
/// successful one — a marker left behind would blame this slot for whatever
/// times out next.
pub fn clear_slot() {
    pac::WATCHDOG.scratch0().write_value(0);
}

/// Run an untrusted native call with `slot` marked as the one in control.
///
/// Bracketing each call rather than the surrounding function is what keeps this
/// exit-safe: the only way to leave without clearing is the call never
/// returning, which is exactly the case the marker exists to record.
pub fn guarding<T>(slot: u8, call: impl FnOnce() -> T) -> T {
    mark_slot(slot);
    let result = call();
    clear_slot();
    result
}

/// The slot to blame for a watchdog reset, if this boot is one.
///
/// Returns `None` for a power-on or a forced reset, so the tag is only ever
/// honoured when the watchdog actually fired. Does not clear the marker:
/// the caller clears it once the quarantine is safely persisted, so a failure
/// in between still leaves evidence for the next boot.
pub fn timed_out_slot(watchdog: &mut Watchdog) -> Option<u8> {
    if watchdog.reset_reason() != Some(ResetReason::TimedOut) {
        return None;
    }
    let raw = watchdog.get_scratch(0);
    ((raw & SLOT_TAG_MASK) == SLOT_TAG_MAGIC).then_some((raw & 0xF) as u8)
}
