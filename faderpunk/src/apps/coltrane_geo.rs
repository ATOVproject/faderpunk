#![allow(dead_code)]

//! Coltrane Changes geometry: tonal center cycles, approach patterns, chord
//! building. Shared by Giant Steps and Axis Matrix.

use crate::apps::groove::{device_swing_permille, device_swing_reverses};
use heapless::Vec;
use libfp::Color;
use smart_leds::RGB8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChordQuality {
    Maj7,
    Dom7,
    Min7,
}

/// How much the harmony is allowed to breathe. Each level adds to the one
/// below it; `Straight` is the untouched engine.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Motion {
    Straight,
    Rubato,
    Sheets,
    Free,
}

pub const MOTION_LABELS: &[&str] = &["Straight", "Rubato", "Sheets", "Free"];

impl Motion {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            1 => Motion::Rubato,
            2 => Motion::Sheets,
            3 => Motion::Free,
            _ => Motion::Straight,
        }
    }
}

/// How many divisions a chord of this function occupies. The tonic that
/// resolves a center leans, the approach chords keep passing.
pub fn step_div_mult(motion: Motion, quality: ChordQuality) -> u32 {
    if motion >= Motion::Rubato && quality == ChordQuality::Maj7 {
        2
    } else {
        1
    }
}

/// Tritone substitution: roughly one dominant in three gets the root a tritone
/// away, quality untouched. Tonics and approach ii chords are never swapped.
pub fn tritone_sub_root(root_midi: u8, quality: ChordQuality, motion: Motion, roll: u16) -> u8 {
    if motion < Motion::Free || quality != ChordQuality::Dom7 || roll >= 1365 {
        return root_midi;
    }
    if root_midi <= 121 {
        root_midi + 6
    } else {
        root_midi - 6
    }
}

/// Ascending-then-descending index run over a chord of `len` notes.
pub fn arp_order(len: usize) -> Vec<u8, 8> {
    let mut out: Vec<u8, 8> = Vec::new();
    for i in 0..len {
        let _ = out.push(i as u8);
    }
    for i in (1..len.saturating_sub(1)).rev() {
        let _ = out.push(i as u8);
    }
    out
}

#[derive(Clone, Copy)]
pub struct CycleStep {
    pub center: u8,
    pub root_offset: u8,
    pub quality: ChordQuality,
    /// Extra division boundaries this chord swallows because the following
    /// slots on the fixed 9-slot grid are empty (it sustains through them).
    pub hold: u8,
}

/// Build the ordered Coltrane cycle on a fixed 9-slot metric grid.
///
/// `interval` is the semitone gap between tonal centers (3 = minor 3rd,
/// 4 = major 3rd, etc.). The grid is always ii(c), V(c), I(c) for each of the
/// three centers. `density` (0..=6) fades approach chords in one at a time:
/// I is always present, V(c) appears once `density >= c + 1`, ii(c) once
/// `density >= 4 + c`. That yields cycle lengths 3,4,5,6,7,8,9.
///
/// Empty slots do not shorten the cycle: they are folded cyclically into the
/// `hold` of the preceding emitted chord (leading empties wrap onto the last
/// chord), so `sum(1 + hold) == 9` always.
pub fn build_cycle(interval: u8, density: u8) -> Vec<CycleStep, 9> {
    let mut out: Vec<CycleStep, 9> = Vec::new();
    let interval = interval.max(1);
    let density = density.min(6);
    let mut leading_empty = 0u8;

    for c in 0u8..3 {
        let center_offset = (c as u16 * interval as u16 % 12) as u8;

        let slots = [
            (
                density >= 4 + c,
                (center_offset + 2) % 12,
                ChordQuality::Min7,
            ),
            (density > c, (center_offset + 7) % 12, ChordQuality::Dom7),
            (true, center_offset, ChordQuality::Maj7),
        ];

        for (present, root_offset, quality) in slots {
            if present {
                let _ = out.push(CycleStep {
                    center: c,
                    root_offset,
                    quality,
                    hold: 0,
                });
            } else if let Some(last) = out.last_mut() {
                last.hold += 1;
            } else {
                leading_empty += 1;
            }
        }
    }

    if let Some(last) = out.last_mut() {
        last.hold += leading_empty;
    }
    out
}

/// Build MIDI note numbers for a chord.
///
/// Voicings: 0 = close triad, 1 = close 7th, 2 = open/drop-2 7th,
/// 3 = quartal (stacked 4ths).
pub fn build_coltrane_chord(root_midi: u8, quality: ChordQuality, voicing: usize) -> Vec<u8, 8> {
    let mut out: Vec<u8, 8> = Vec::new();
    let r = root_midi as i16;

    let intervals: &[i16] = match (quality, voicing) {
        (_, 3) => &[0, 5, 10, 15],
        (ChordQuality::Maj7, 0) => &[0, 4, 7],
        (ChordQuality::Dom7, 0) => &[0, 4, 7],
        (ChordQuality::Min7, 0) => &[0, 3, 7],
        (ChordQuality::Maj7, 1) => &[0, 4, 7, 11],
        (ChordQuality::Dom7, 1) => &[0, 4, 7, 10],
        (ChordQuality::Min7, 1) => &[0, 3, 7, 10],
        (ChordQuality::Maj7, _) => &[0, 7, 11, 16],
        (ChordQuality::Dom7, _) => &[0, 7, 10, 16],
        (ChordQuality::Min7, _) => &[0, 7, 10, 15],
    };

    for &iv in intervals {
        let n = r + iv;
        if (0..=127).contains(&n) {
            let _ = out.push(n as u8);
        }
    }
    out
}

/// Chord voiced to minimise movement from the previous voicing.
///
/// Candidates are whole-chord octave shifts crossed with inversions; each is
/// scored by the total semitone distance to the nearest note of `prev`. Empty
/// `prev` falls back to `build_coltrane_chord`.
pub fn build_chord_voice_led(
    root_midi: u8,
    quality: ChordQuality,
    voicing: usize,
    prev: &[u8],
) -> Vec<u8, 8> {
    let base = build_coltrane_chord(root_midi, quality, voicing);
    if prev.is_empty() || base.is_empty() {
        return base;
    }

    let mut best = base.clone();
    let mut best_cost = u32::MAX;
    let mut cand: Vec<u8, 8> = Vec::new();

    // Neutral candidate comes first so ties keep the plain voicing.
    for &oct in &[0i16, -12, 12, -24, 24] {
        for inv in 0..base.len() {
            cand.clear();
            let mut fits = true;
            for (i, &n) in base.iter().enumerate() {
                let v = n as i16 + oct + if i < inv { 12 } else { 0 };
                if !(0..=127).contains(&v) {
                    fits = false;
                    break;
                }
                let _ = cand.push(v as u8);
            }
            if !fits {
                continue;
            }

            let mut cost = 0u32;
            for &n in cand.iter() {
                let mut d = u32::MAX;
                for &p in prev {
                    d = d.min(u32::from((n as i16 - p as i16).unsigned_abs()));
                }
                cost += d;
            }
            if cost < best_cost {
                best_cost = cost;
                best = cand.clone();
            }
        }
    }
    best
}

/// Feel-scaled velocity (12-bit). `feel` is 0..=4095; at 0 the base velocity is
/// returned untouched. `strong` marks the tonic of a center, which gets
/// accented while approach chords lighten. `roll` is a 0..=4095 die roll
/// driving the humanised variation.
pub fn feel_velocity(base_vel12: u16, feel: u16, strong: bool, roll: u16) -> u16 {
    if feel == 0 {
        return base_vel12;
    }
    let b = base_vel12 as i32;
    let s = (feel.min(4095) as i32 * 255) / 4095;
    // Per-mille of the base velocity at full Feel.
    let shape = if strong { 400 } else { -500 };
    let mut v = b + (b * shape * s) / (1000 * 255);
    let jitter = ((roll.min(4095) as i32 * 2000) / 4095) - 1000;
    let jb = (b * jitter) / 1000;
    v += (jb * 8 * s) / (100 * 255);
    v.clamp(1, 4095) as u16
}

/// MPC-style swing delay in clock ticks for `step`: one parity gets pushed back
/// by up to a third of the division at full Feel. That third is a budget shared
/// with the device clock — whatever the global swing already displaces is
/// subtracted — and a negative `swing_amount` flips which parity is delayed so
/// Feel leans with the clock instead of against it.
pub fn feel_swing_ticks(feel: u16, div_ticks: u32, step: u32, swing_amount: i8) -> u32 {
    if feel == 0 || div_ticks < 2 {
        return 0;
    }
    let delay_this = if device_swing_reverses(swing_amount) {
        step.is_multiple_of(2)
    } else {
        !step.is_multiple_of(2)
    };
    if !delay_this {
        return 0;
    }
    let budget_permille = 333u32.saturating_sub(device_swing_permille(div_ticks, swing_amount));
    if budget_permille == 0 {
        return 0;
    }
    // div_ticks <= 96, budget <= 333, feel <= 4095 → well inside u32.
    let d = (div_ticks * budget_permille * feel.min(4095) as u32) / (4095 * 1000);
    d.min(div_ticks - 1)
}

/// Feedback color for the Interval cycle gesture, one per index.
pub fn interval_color(idx: u8) -> Color {
    match idx % 4 {
        0 => Color::Cyan,
        1 => Color::Green,
        2 => Color::Yellow,
        _ => Color::Rose,
    }
}

/// Fixed RGB triad (legacy): Blue / Green / Orange at ~120 deg.
pub fn center_color(center_idx: u8) -> (u8, u8, u8) {
    match center_idx % 3 {
        0 => (40, 80, 220),
        1 => (30, 200, 80),
        _ => (230, 120, 20),
    }
}

/// Tonal-center color: rotate the 120 deg triad so center 0 matches `base` hue.
/// Centers stay equally spaced (+0 / +120 / +240) on the wheel.
pub fn center_from_app(base: Color, center_idx: u8) -> Color {
    let RGB8 { r, g, b } = RGB8::from(base);
    let (h, s, v) = rgb_to_hsv(r, g, b);
    // Near-white / grey: fall back to fixed triad so centers stay distinct.
    if s < 20 {
        let (cr, cg, cb) = center_color(center_idx);
        return Color::Custom(cr, cg, cb);
    }
    let h2 = (h + u16::from(center_idx % 3) * 120) % 360;
    let (nr, ng, nb) = hsv_to_rgb(h2, s.max(140), v.max(160));
    Color::Custom(nr, ng, nb)
}

/// Tonal-center color swayed by `degrees` on the wheel, to encode how far the
/// sounding chord sits from its center's own tonic.
pub fn function_hue(base: Color, center_idx: u8, degrees: u16) -> Color {
    let RGB8 { r, g, b } = RGB8::from(base);
    let (h, s, v) = rgb_to_hsv(r, g, b);
    if s < 20 {
        let (cr, cg, cb) = center_color(center_idx);
        return Color::Custom(cr, cg, cb);
    }
    let h2 = (h + u16::from(center_idx % 3) * 120 + degrees) % 360;
    let (nr, ng, nb) = hsv_to_rgb(h2, s.max(140), v.max(160));
    Color::Custom(nr, ng, nb)
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let v = max;
    if max == 0 {
        return (0, 0, 0);
    }
    let d = max - min;
    let s = ((u16::from(d) * 255) / u16::from(max)) as u8;
    if d == 0 {
        return (0, 0, v);
    }
    let (r, g, b, max, d) = (
        i32::from(r),
        i32::from(g),
        i32::from(b),
        i32::from(max),
        i32::from(d),
    );
    let h = if max == r {
        ((g - b) * 60) / d
    } else if max == g {
        120 + ((b - r) * 60) / d
    } else {
        240 + ((r - g) * 60) / d
    };
    ((h.rem_euclid(360)) as u16, s, v)
}

fn hsv_to_rgb(h: u16, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }
    let h = h % 360;
    let sector = h / 60;
    let f = h % 60;
    let v = u16::from(v);
    let s = u16::from(s);
    let p = v * (255 - s) / 255;
    let q = v * (255 - (s * f) / 60) / 255;
    let t = v * (255 - (s * (60 - f)) / 60) / 255;
    let (r, g, b) = match sector {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    (r as u8, g as u8, b as u8)
}

/// Brightness (0-255) that peaks when fader position aligns with a center and
/// dims as it drifts away. `fader_pos` is 0-4095, mapped across `num_centers`.
pub fn center_brightness(fader_pos: u16, center_idx: u8, num_centers: u8) -> u8 {
    let nc = num_centers.max(1) as u32;
    let center_pos = (center_idx as u32 * 4095) / nc.max(1);
    let dist = (fader_pos as i32 - center_pos as i32).unsigned_abs();
    let span = 4095u32 / nc;
    if dist >= span {
        0
    } else {
        (255 * (span - dist) / span) as u8
    }
}
