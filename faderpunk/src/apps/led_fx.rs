//! Shared LED color helpers (spectrum hue, genre-axis math).
//!
//! Open spectrum is red→blue (~0°…240°) — no magenta wrap (not a spectral color).

use libfp::Color;

/// Integer HSV→RGB with S=V=max. Hue in degrees (0..360).
pub fn hsv_to_rgb(hue: u16) -> (u8, u8, u8) {
    let hue = hue % 360;
    let sector = hue / 60; // 0..=5
    let ramp = ((hue % 60) as u32 * 255 / 59) as u8;
    match sector {
        0 => (255, ramp, 0),
        1 => (255 - ramp, 255, 0),
        2 => (0, 255, ramp),
        3 => (0, 255 - ramp, 255),
        4 => (ramp, 0, 255),
        _ => (255, 0, 255 - ramp),
    }
}

/// Max hue for the open spectrum (red→blue). Magenta/wrap excluded.
pub const SPECTRUM_HUE_MAX: u16 = 240;

/// u12 `0..=4095` → [`Color`] along open spectrum (red→yellow→green→cyan→blue).
pub fn spectrum_color(pos: u16) -> Color {
    let pos = pos.min(4095) as u32;
    let hue = ((pos * u32::from(SPECTRUM_HUE_MAX)) / 4095) as u16;
    let (r, g, b) = hsv_to_rgb(hue);
    Color::Custom(r, g, b)
}

/// Continuous genre axis: fader `0..=4095` → `(lo, hi, frac_u8)` across `picks-1` spans.
///
/// `frac` is `0..=255` between `lo` and `hi`. At the ends `lo == hi` and `frac == 0`.
pub fn genre_pair(fader: u16, picks: usize) -> (usize, usize, u8) {
    let picks = picks.max(1);
    if picks == 1 {
        return (0, 0, 0);
    }
    let spans = (picks - 1) as u32;
    let f = u32::from(fader.min(4095));
    // Fixed-point position along 0..spans
    let scaled = f * spans;
    let lo = (scaled / 4095) as usize;
    let lo = lo.min(picks - 1);
    if lo >= picks - 1 {
        return (picks - 1, picks - 1, 0);
    }
    let rem = scaled % 4095;
    let frac = ((rem * 255) / 4095) as u8;
    (lo, lo + 1, frac)
}

/// Nearest genre index on the continuous axis (midpoint snap).
#[allow(dead_code)] // used by Grooves; kept public for shared genre-axis API
pub fn genre_nearest(fader: u16, picks: usize) -> usize {
    let (lo, hi, frac) = genre_pair(fader, picks);
    if frac < 128 {
        lo
    } else {
        hi
    }
}

/// Integer lerp `a → b` by `frac` (`0..=255`).
pub fn lerp_i32(a: i32, b: i32, frac: u8) -> i32 {
    a + ((b - a) * i32::from(frac)) / 255
}

/// Integer lerp for `u8` amounts.
pub fn lerp_u8(a: u8, b: u8, frac: u8) -> u8 {
    lerp_i32(i32::from(a), i32::from(b), frac).clamp(0, 255) as u8
}
