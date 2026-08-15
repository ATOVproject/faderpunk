//! Shared swing / feel math for Grooves and Chord Vamp.
//!
//! Genre **labels/colors** live in [`super::genre_palette`]; drum/harmony DNA
//! stays app-local. This module only owns timing helpers and per-genre swing bias.

use super::genre_palette::NUM_GENRES;

/// 24 PPQN → one 16th note.
pub const SIXTEENTH: u32 = 6;

/// Flat core velocity % when Feel is fully attenuated (all voices equal).
/// Used by Grooves; kept public for the shared API.
#[allow(dead_code)]
pub const FLAT_VEL: u16 = 70;

/// Per-genre default swing % (0–100); order matches genre_palette morph axis.
pub const SWING_BIAS: [i8; NUM_GENRES] = [
    20, // Dub
    35, // Disco
    30, // House
    8,  // Techno — stays straighter by DNA, not by burying Feel
    45, // Trip-Hop
    40, // Hip-Hop
    48, // Jungle
    50, // UK Garage
    25, // Dubstep
];

/// Ease-in Feel curve: lower third stays near-flat, upper half ramps hard.
/// Used for swing / character blend — keep this shape for Vamp / Bassment.
#[allow(dead_code)]
#[inline]
pub fn feel_curve(feel: u16) -> u16 {
    let f = u32::from(feel);
    ((f * f) / 4095) as u16
}

/// Softer Feel curve for humanization (jitter, ghost chance). Midpoint of
/// linear and quadratic so the lower fader half is audible without changing
/// [`feel_curve`] (shared with Vamp / Bassment).
#[allow(dead_code)]
#[inline]
pub fn humanize_curve(feel: u16) -> u16 {
    let f = u32::from(feel);
    ((f * (f + 4095)) / (2 * 4095)) as u16
}

/// Linear blend `flat → character` by curved Feel amount (0..=4095).
#[allow(dead_code)]
#[inline]
pub fn feel_lerp_u16(flat: u16, character: u16, feel: u16) -> u16 {
    let t = u32::from(feel_curve(feel));
    let flat = u32::from(flat);
    let character = u32::from(character);
    if character >= flat {
        (flat + (character - flat) * t / 4095) as u16
    } else {
        (flat - (flat - character) * t / 4095) as u16
    }
}

/// Signed blend for microtiming offsets (ms or ticks — caller chooses units).
#[allow(dead_code)]
#[inline]
pub fn feel_lerp_i32(flat: i32, character: i32, feel: u16) -> i32 {
    let t = i32::from(feel_curve(feel));
    flat + ((character - flat) * t) / 4095
}

/// MPC-style: delay odd 16ths by `0..=(SIXTEENTH-1)` scaled by `swing_pct` (0–100).
/// `reversed` flips which parity is delayed. Kept for Vamp / Bassment (tick domain).
#[allow(dead_code)]
#[inline]
pub fn swing_delay_ticks(step: u32, swing_pct: i32, reversed: bool) -> u32 {
    let pct = swing_pct.clamp(0, 100) as u32;
    if pct == 0 {
        return 0;
    }
    let odd = step % 2 == 1;
    let delay_this = if reversed { !odd } else { odd };
    if !delay_this {
        return 0;
    }
    let max_delay = SIXTEENTH.saturating_sub(1).max(1);
    ((max_delay * pct) / 100).min(max_delay)
}

/// Continuous swing in milliseconds. `swing_pct = 50` ≈ triplet swing
/// (⅓ of a 16th); 100 = ⅔. Same genre-bias scale as [`swing_delay_ticks`].
#[allow(dead_code)]
#[inline]
pub fn swing_delay_ms(step: u32, swing_pct: i32, reversed: bool, sixteenth_ms: u32) -> u32 {
    let pct = swing_pct.clamp(0, 100) as u32;
    if pct == 0 || sixteenth_ms == 0 {
        return 0;
    }
    let odd = step % 2 == 1;
    let delay_this = if reversed { !odd } else { odd };
    if !delay_this {
        return 0;
    }
    // 50% → ⅓ of a 16th; 100% → ⅔. Cap just under the next step.
    let raw = (sixteenth_ms * pct * 2) / 300;
    raw.min(sixteenth_ms.saturating_sub(2))
}

/// Half of the device swing window in 24-PPQN ticks. Mirrors the private
/// `SWING_HALF_INTERVAL` in [`crate::tasks::clock`]; keep both in sync.
const DEVICE_SWING_HALF: u32 = 6;

/// Fraction of one grid step, in per-mille, that the device's global swing
/// already displaces. Zero when the app's grid is coarser than the swing
/// window half, because those steps always land on the window anchor and the
/// clock never moves them.
#[allow(dead_code)]
#[inline]
pub fn device_swing_permille(div_ticks: u32, swing_amount: i8) -> u32 {
    if div_ticks > DEVICE_SWING_HALF || swing_amount == 0 {
        return 0;
    }
    // The clock shifts the offbeat by `H * s / 50` ticks, i.e. |s|/50 of a 16th.
    (swing_amount.unsigned_abs() as u32 * 1000) / 50
}

/// Threshold below which a negative global swing is treated as straight for
/// direction purposes. Flipping parity is a large musical change, so it should
/// not fall out of a value that barely moves the grid at all.
const DEVICE_SWING_DIRECTION_MIN: i8 = 8;

/// Whether the app should flip which side of the grid it delays, so its own
/// swing leans the same way the device clock already does instead of pulling
/// against it.
#[allow(dead_code)]
#[inline]
pub fn device_swing_reverses(swing_amount: i8) -> bool {
    swing_amount <= -DEVICE_SWING_DIRECTION_MIN
}

/// Genre swing bias as 0–100.
#[inline]
pub fn swing_bias(genre: usize) -> u8 {
    SWING_BIAS[genre.min(NUM_GENRES - 1)].clamp(0, 100) as u8
}
