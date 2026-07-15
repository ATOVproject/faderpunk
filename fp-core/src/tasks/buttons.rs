//! Shared button state. The firmware's GPIO scanning task (and the
//! simulator's virtual buttons) write these atomics and publish the
//! corresponding `InputEvent`s.

use portable_atomic::{AtomicBool, Ordering};

pub static BUTTON_PRESSED: [AtomicBool; 18] = [const { AtomicBool::new(false) }; 18];

#[inline(always)]
pub fn is_channel_button_pressed(channel: usize) -> bool {
    BUTTON_PRESSED[channel.clamp(0, 15)].load(Ordering::Relaxed)
}

#[inline(always)]
pub fn is_shift_button_pressed() -> bool {
    BUTTON_PRESSED[17].load(Ordering::Relaxed)
}

#[inline(always)]
pub fn is_scene_button_pressed() -> bool {
    BUTTON_PRESSED[16].load(Ordering::Relaxed)
}
