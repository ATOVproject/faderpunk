//! Shared "follow the device tonality" helpers for note-generating apps.
//!
//! The device has one global Key + Tonic, reachable live by holding the Scene
//! button (Fader 4 = Scale, Fader 5 = Tonic). An app that follows the Tonic
//! turns that fader into a **transpose** — and every following app moves
//! together, which is what you want when a bass and a melody play the same
//! tune. Apps expose this as two `Param::bool`s named "Follow device tonic"
//! and "Follow device scale".
//!
//! Scale and Tonic are deliberately separate: an app's Scale is often part of
//! its own character (Contura's Folk set, Bassment's genre feel), so the usual
//! default is to follow the Tonic but keep the local Scale.
//!
//! **Cost:** every call here copies the whole `GlobalConfig`. Resolve once per
//! bar or per phrase and cache it — never per step and never per note.

// Each app needs a different part of this surface, and the app feature branches
// carry one app each — so on those, some of it is unused by construction.
#![allow(dead_code)]

use libfp::{Key, MidiNote};
use midly::num::u7;

use crate::tasks::global_config::get_global_config;

/// The device Scale, normalized for note generators: a Key of `Off` means
/// "don't quantize" device-wide, but a generator has to pick notes from
/// *something*, so it reads as chromatic here.
pub fn device_key() -> Key {
    match get_global_config().quantizer.key {
        Key::Off => Key::Chromatic,
        k => k,
    }
}

/// Pitch class (0–11) the app should anchor on: the device Tonic when
/// following, otherwise the pitch class of the app's own root.
pub fn tonic_pc(follow: bool, local_root: MidiNote) -> u8 {
    if follow {
        get_global_config().quantizer.tonic as u8
    } else {
        midi_u8(local_root) % 12
    }
}

/// The app's root, retuned onto the device Tonic when following. This is all an
/// app needs whose scale already comes from the global quantizer — there the
/// scale follows anyway and only the root runs loose.
pub fn root(follow: bool, local_root: MidiNote) -> u8 {
    if follow {
        retune(local_root, get_global_config().quantizer.tonic as u8)
    } else {
        midi_u8(local_root)
    }
}

/// Root *and* Scale at once, for apps whose scale is a plain [`Key`] — one
/// `GlobalConfig` copy instead of two.
///
/// The returned root keeps the octave of `local_root` and only takes on the
/// new pitch class, so following the Tonic transposes within the register the
/// patch was written in rather than jumping the app an octave.
pub fn root_and_key(
    follow_tonic: bool,
    follow_scale: bool,
    local_root: MidiNote,
    local_key: Key,
) -> (u8, Key) {
    if !follow_tonic && !follow_scale {
        return (midi_u8(local_root), local_key);
    }
    let c = get_global_config();
    let pc = if follow_tonic {
        c.quantizer.tonic as u8
    } else {
        midi_u8(local_root) % 12
    };
    let key = if follow_scale {
        match c.quantizer.key {
            Key::Off => Key::Chromatic,
            k => k,
        }
    } else {
        local_key
    };
    (retune(local_root, pc), key)
}

/// Re-anchor an absolute root note onto pitch class `pc`, keeping its octave.
pub fn retune(root: MidiNote, pc: u8) -> u8 {
    let n = midi_u8(root);
    let target = (n - n % 12) + pc % 12;
    // A root in the top octave can overshoot; drop an octave rather than clamp,
    // so the pitch class stays the one that was asked for.
    if target > 127 {
        target - 12
    } else {
        target
    }
}

fn midi_u8(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}
