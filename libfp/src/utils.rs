use embassy_time::Duration;
use midly::num::u7;

use crate::Curve;

pub const fn bpm_to_clock_duration(bpm: f32, ppqn: u8) -> Duration {
    Duration::from_nanos((1_000_000_000.0 / (bpm as f64 / 60.0 * ppqn as f64)) as u64)
}

/// Scale from 4095 u16 to 127 u7
pub fn scale_bits_12_7(value: u16) -> u7 {
    u7::new(((value as u32 * 127) / 4095) as u8)
}

/// Scale from 127 u7 to 4095 u16
pub fn scale_bits_7_12(value: u7) -> u16 {
    ((value.as_int() as u32 * 4095) / 127) as u16
}

/// Scale from 4095 (12-bit) to 16383 (14-bit)
pub fn scale_bits_12_14(value: u16) -> u16 {
    ((value as u32 * 16383) / 4095) as u16
}

/// Scale from 16383 (14-bit) to 4095 (12-bit)
pub fn scale_bits_14_12(value: u16) -> u16 {
    ((value as u32 * 4095) / 16383) as u16
}

/// Convert u7 into u16
pub fn bits_7_16(value: u7) -> u16 {
    value.as_int() as u16
}

/// Split 0 to 4095 value to two 0-255 u8 used for LEDs
pub fn split_unsigned_value(input: u16) -> [u8; 2] {
    let clamped = input.clamp(0, 4095);
    if clamped <= 2047 {
        let neg = ((2047 - clamped) / 8).clamp(0, 255) as u8;
        [0, neg]
    } else {
        let pos = ((clamped - 2047) / 8).clamp(0, 255) as u8;
        [pos, 0]
    }
}

/// Split -2047 2047 value to two 0-255 u8 used for LEDs
pub fn split_signed_value(input: i32) -> [u8; 2] {
    let clamped = input.clamp(-2047, 2047);
    if clamped >= 0 {
        let pos = ((clamped * 255 + 1023) / 2047).clamp(0, 255) as u8;
        [pos, 0]
    } else {
        let neg = (((-clamped) * 255 + 1023) / 2047).clamp(0, 255) as u8;
        [0, neg]
    }
}

/// Attenuate a u12 by another u12
pub fn attenuate(signal: u16, level: u16) -> u16 {
    let attenuated = (signal as u32 * level as u32) / 4095;

    attenuated as u16
}

/// Rescale a 12-bit value (`0..=4095`) into a `min..=max` interval.
pub fn rescale_12bit_int(input: u16, min: u16, max: u16) -> u16 {
    let input = input.min(4095);

    if min >= max {
        return min;
    }

    let range = max - min;
    min + attenuate(range, input)
}

/// Clock divider resolution table for selectable division modes.
pub fn resolution_for_mode(mode: usize) -> &'static [u16] {
    match mode {
        0 => &[384, 192, 96, 48, 24, 12, 6, 3],
        1 => &[384, 192, 96, 48, 24, 16, 8, 4, 2],
        _ => &[384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2],
    }
}

/// Map a 12-bit value to an index into a slice of the given length.
pub fn value_to_index(value: u16, len: usize) -> usize {
    ((value as usize * len) / 4096).min(len.saturating_sub(1))
}

/// Map a 12-bit value to a resolution from the given table.
pub fn value_to_resolution(value: u16, resolution: &[u16]) -> u32 {
    resolution[value_to_index(value, resolution.len())] as u32
}

/// Map a 12-bit value to a resolution, offset by a bipolar CV input.
pub fn resolution_with_input_offset(base: u16, in_val: u16, resolution: &[u16]) -> u32 {
    let base_index = value_to_index(base, resolution.len()) as i32;
    let max_offset = ((resolution.len() as i32 - 1) / 2).max(1);
    let offset = ((in_val as i32 - 2047) * max_offset / 2047).clamp(-max_offset, max_offset);
    let index = (base_index + offset).clamp(0, (resolution.len() - 1) as i32) as usize;
    resolution[index] as u32
}

/// Use to attenuate 0-4095 representing a bipolar value
pub fn attenuate_bipolar(signal: u16, level: u16) -> u16 {
    let center = 2048u32;

    // Convert to signed deviation from center
    let deviation = signal as i32 - center as i32;

    // Apply attenuation as fixed-point scaling
    let scaled = (deviation as i64 * level as i64) / 4095;

    // Add back the center and clamp to 0..=4095
    let result = center as i64 + scaled;
    result.clamp(0, 4095) as u16
}

/// Attenuverter
pub fn attenuverter(input: u16, modulation: u16) -> u16 {
    let input = input as i32;
    let mod_val = modulation as i32;

    // Map modulation (0..=4095) to a blend factor from -1.0 (invert) to +1.0 (normal)
    let blend = (mod_val - 2047) as f32 / 2048.0;

    // Normal = input, Inverted = 4095 - input
    let normal = input as f32;
    let inverted = (4095 - input) as f32;

    // Interpolate between inverted and normal
    let result = inverted * (1.0 - blend) / 2.0 + normal * (1.0 + blend) / 2.0;

    result.clamp(0.0, 4095.0) as u16
}

/// Slew limiter
pub fn slew_limiter(prev: f32, input: u16, rise_rate: u16, fall_rate: u16) -> f32 {
    let curve = Curve::Exponential;
    let min_slew = 50.0;
    let max_slew = 0.5;
    let delta = input as i32 - prev as i32;
    if delta > 0 {
        let step = curve.at(4095 - rise_rate) as f32 / min_slew + max_slew;
        if step < (4095.0 / min_slew + max_slew) - 10.0 {
            if prev + step < input as f32 {
                prev + step
            } else {
                input as f32
            }
        } else {
            input.clamp(0, 4095) as f32
        }
    } else if delta < 0 {
        let step = curve.at(4095 - fall_rate) as f32 / min_slew + max_slew;
        if step < (4095.0 / min_slew + max_slew) - 10.0 {
            if prev - step > input as f32 {
                prev - step
            } else {
                input as f32
            }
        } else {
            input.clamp(0, 4095) as f32
        }
    } else {
        input.clamp(0, 4095) as f32
    }
}

pub fn slew_2(prev: u16, input: u16, slew: u16, snap: i32) -> u16 {
    let smoothed = ((prev as u32 * slew as u32 + input as u32) / (slew as u32 + 1)) as u16;

    if (smoothed as i32 - input as i32).abs() < snap {
        input
    } else {
        smoothed
    }
}

/// Very short slew meant to avoid clicks
pub fn clickless(prev: u16, input: u16) -> u16 {
    // Snap threshold: if the difference is small, jump to input
    if (prev as i32 - input as i32).abs() < 16 {
        input
    } else {
        ((prev as u32 * 15 + input as u32) / 16) as u16
    }
}
