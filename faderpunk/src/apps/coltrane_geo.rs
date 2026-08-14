#![allow(dead_code)]

//! Coltrane Changes geometry: tonal center cycles, approach patterns, chord
//! building. Shared by Giant Steps and Axis Matrix.

use heapless::Vec;
use libfp::Color;
use smart_leds::RGB8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChordQuality {
    Maj7,
    Dom7,
    Min7,
}

#[derive(Clone, Copy)]
pub struct CycleStep {
    pub center: u8,
    pub root_offset: u8,
    pub quality: ChordQuality,
}

/// Build the ordered Coltrane cycle.
///
/// `interval` is the semitone gap between tonal centers (3 = minor 3rd,
/// 4 = major 3rd, etc.). `density` selects approach depth:
/// 0 = centers only (3 steps), 1 = V-I (6), 2 = ii-V-I (9).
pub fn build_cycle(interval: u8, density: u8) -> Vec<CycleStep, 9> {
    let mut out: Vec<CycleStep, 9> = Vec::new();
    let interval = interval.max(1);

    for c in 0u8..3 {
        let center_offset = (c as u16 * interval as u16 % 12) as u8;

        if density >= 2 {
            let ii_offset = (center_offset + 12 - 2) % 12;
            let _ = out.push(CycleStep {
                center: c,
                root_offset: ii_offset,
                quality: ChordQuality::Min7,
            });
        }
        if density >= 1 {
            let v_offset = (center_offset + 7) % 12;
            let _ = out.push(CycleStep {
                center: c,
                root_offset: v_offset,
                quality: ChordQuality::Dom7,
            });
        }
        let _ = out.push(CycleStep {
            center: c,
            root_offset: center_offset,
            quality: ChordQuality::Maj7,
        });
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
    let (r, g, b, max, d) = (i32::from(r), i32::from(g), i32::from(b), i32::from(max), i32::from(d));
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
