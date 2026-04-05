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
pub fn resolution_for_mode(mode: usize) -> &'static [u32] {
    match mode {
        0 => &[384, 192, 96, 48, 24, 12, 6, 3],
        1 => &[384, 192, 96, 48, 24, 16, 8, 4, 2],
        _ => &[384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2],
    }
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

/// Opaque state for [`slew_lin`] and [`slew_exp`].
///
/// Stores a Q8 fixed-point value internally. Use [`SlewState::value`] to read
/// the current 12-bit output. Both slew functions share this type, so their
/// states are interchangeable — you can switch between linear and exponential
/// mid-stream without resetting.
#[derive(Clone, Copy, Default)]
pub struct SlewState(u32);

impl SlewState {
    pub fn new() -> Self {
        Self(0)
    }

    /// Returns the current output as a plain 12-bit value.
    pub fn value(self) -> u16 {
        (self.0 >> 8) as u16
    }
}

impl From<u16> for SlewState {
    fn from(v: u16) -> Self {
        Self((v as u32) << 8)
    }
}

/// Linear slew with independent rise and fall rates.
///
/// Advances toward `input` by a fixed step per tick, giving a constant-rate
/// (linear) response. Step size is derived from an exponential curve applied
/// to `rise_rate`/`fall_rate`, so the control feels logarithmic to the user.
/// Bypasses slew entirely when the rate is at maximum.
pub fn slew_lin(prev: SlewState, input: u16, rise_rate: u16, fall_rate: u16) -> SlewState {
    let curve = Curve::Exponential;
    let prev = prev.0;
    let input_fp = (input as u32) << 8;
    // Bypass threshold in Q8: equivalent to (4095/50 + 0.5 - 10.0) * 256 = 18534
    let bypass_fp: u32 = 4095 * 256 / 50 + 128 - 10 * 256;

    SlewState(if input_fp > prev {
        let step_fp = curve.at(4095 - rise_rate) as u32 * 256 / 50 + 128;
        if step_fp < bypass_fp {
            if prev + step_fp < input_fp { prev + step_fp } else { input_fp }
        } else {
            input_fp
        }
    } else if input_fp < prev {
        let step_fp = curve.at(4095 - fall_rate) as u32 * 256 / 50 + 128;
        if step_fp < bypass_fp {
            if prev.saturating_sub(step_fp) > input_fp { prev - step_fp } else { input_fp }
        } else {
            input_fp
        }
    } else {
        input_fp
    })
}

/// Exponential lag with independent rise and fall time constants.
///
/// # Fixed-point state
///
/// The return value is **not** a plain 12-bit value — it carries 8 fractional
/// bits in its lower byte to track sub-integer position between calls.
/// Store it as `u32` and pass it back as `prev` each tick.
/// To get the actual output value, shift right by 8:
///
/// ```ignore
/// let mut state = SlewState::new();
/// // called every tick:
/// state = slew_exp(state, input, slew_rise, slew_fall);
/// let output = state.value(); // 12-bit value, ready for DAC/MIDI/LEDs
/// ```
pub fn slew_exp(prev: SlewState, input: u16, slew_rise: u16, slew_fall: u16) -> SlewState {
    let prev = prev.0;
    let input_fp = (input as u32) << 8;
    let slew = if input_fp > prev { slew_rise } else { slew_fall };
    let smoothed = (prev * slew as u32 + input_fp) / (slew as u32 + 1);
    let snap = (slew >> 8) + 1;

    SlewState(if ((smoothed >> 8) as u16).abs_diff(input) <= snap {
        input_fp
    } else {
        smoothed
    })
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
