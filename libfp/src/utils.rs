use embassy_time::Duration;
use libm::expf;
use midly::num::u7;

use crate::Curve;

pub const fn bpm_to_clock_duration(bpm: f32, ppqn: u8) -> Duration {
    Duration::from_nanos((1_000_000_000.0 / (bpm as f64 / 60.0 * ppqn as f64)) as u64)
}

/// Scale a 12-bit value (0..=4095) to a 7-bit MIDI value (0..=127).
/// Uses integer division by 32 (4096/128) so each CC step covers exactly
/// 32 input values and CC 127 is reachable for any input >= 4064.
pub fn scale_bits_12_7(value: u16) -> u7 {
    u7::new((value / 32) as u8)
}

/// Resolution at which a 12-bit value should be de-duplicated before
/// emitting MIDI: full 12-bit in NRPN mode, 7-bit-quantized in CC mode.
pub fn midi_gate(value: u16, nrpn: bool) -> u16 {
    if nrpn {
        value
    } else {
        scale_bits_12_7(value).as_int() as u16
    }
}

/// Scale from 4095 u16 to 255 u8
pub fn scale_bits_12_8(value: u16) -> u8 {
    ((value as u32 * 255) / 4095) as u8
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

pub fn slew_2(prev: u16, input: u16, slew: u16, snap: i32) -> u16 {
    let smoothed = ((prev as u32 * slew as u32 + input as u32) / (slew as u32 + 1)) as u16;

    if (smoothed as i32 - input as i32).abs() < snap {
        input
    } else {
        smoothed
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
/// Bypasses slew entirely when the rate is at its minimum (0).
pub fn slew_lin(prev: SlewState, input: u16, rise_rate: u16, fall_rate: u16) -> SlewState {
    let curve = Curve::Exponential;
    let prev = prev.0;
    let input_fp = (input as u32) << 8;
    // Bypass threshold in Q8: equivalent to (4095/50 + 0.5 - 10.0) * 256 = 18534
    let bypass_fp: u32 = 4095 * 256 / 50 + 128 - 10 * 256;

    let step_toward = |rate: u16| -> u32 { curve.at(4095 - rate) as u32 * 256 / 50 + 128 };

    SlewState(if input_fp > prev {
        let step_fp = step_toward(rise_rate);
        if step_fp < bypass_fp && prev + step_fp < input_fp {
            prev + step_fp
        } else {
            input_fp
        }
    } else if input_fp < prev {
        let step_fp = step_toward(fall_rate);
        if step_fp < bypass_fp && prev.saturating_sub(step_fp) > input_fp {
            prev - step_fp
        } else {
            input_fp
        }
    } else {
        input_fp
    })
}

/// Exponential lag with independent rise and fall time constants.
///
/// Pass the returned `SlewState` back in as `prev` each tick, and call
/// [`SlewState::value`] to read the current 12-bit output:
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
    let slew = if input_fp > prev {
        slew_rise
    } else {
        slew_fall
    };
    let smoothed = (prev * slew as u32 + input_fp) / (slew as u32 + 1);
    let snap = (slew >> 8) + 1;

    SlewState(if ((smoothed >> 8) as u16).abs_diff(input) <= snap {
        input_fp
    } else {
        smoothed
    })
}

/// Rotate a bit pattern left within a given bit width
pub fn euclidean_rotl(value: u32, width: u8, rotation: u8) -> u32 {
    let rotation = rotation % width;
    ((value << rotation) | (value >> (width - rotation))) & ((1 << width) - 1)
}

/// Return the Bjorklund/Euclidean pattern for `num_beats` in `num_steps` as a bitmask.
/// Bit N is set if step N fires. `rotation` offsets the pattern; `padding` extends the
/// effective width for rotation without changing the number of active steps.
pub fn euclidean_pattern(num_steps: u8, num_beats: u8, rotation: u8, padding: u8) -> u32 {
    use crate::constants::BJORKLUND_PATTERNS;
    let steps = num_steps.max(2);
    let beats = num_beats.min(steps);
    let index = (steps as usize - 2) * 33 + beats as usize;
    let mut pattern = BJORKLUND_PATTERNS.get(index).copied().unwrap_or(0);
    if rotation > 0 {
        let rot = rotation % (steps + padding);
        pattern = euclidean_rotl(pattern, steps + padding, rot);
    }
    pattern
}

/// Return true if `clock` (step count from origin) fires in an E(`num_beats`, `num_steps`) pattern.
pub fn euclidean_at(num_steps: u8, num_beats: u8, rotation: u8, clock: u32) -> bool {
    let pattern = euclidean_pattern(num_steps, num_beats, rotation, 0);
    let pos = (clock % num_steps as u32) as u8;
    (pattern & (1 << pos)) != 0
}

/// Scale the top `x` bits of a 16-bit shift register (`x` in 1..=16) to a 12-bit value
/// (`0..=4095`). Used by the Turing-machine apps to turn register state into a CV/level.
pub fn scale_to_12bit(input: u16, x: u8) -> u16 {
    let x = x.clamp(1, 16);
    let top_x_bits = input >> (16 - x);
    let max_x_val = (1u32 << x) - 1;
    ((top_x_bits as u32 * 4095) / max_x_val) as u16
}

/// Turing-machine shift-register step. Rotates `x` right by one bit, re-injecting the
/// looped bit at the MSB. The looped bit is read from the bottom of the active
/// `length`-bit window (`length` in 1..=16) so the pattern repeats with period `length`;
/// when `a > b` the bit is inverted (the probabilistic mutation).
/// Returns `(new_register, was_flipped, output_bit)`.
pub fn rotate_select_bit(x: u16, a: u16, b: u16, length: u16) -> (u16, bool, bool) {
    let bit_index = (16 - length).clamp(0, 16);
    let original_bit = ((x >> bit_index) & 1) as u8;
    let mut bit = original_bit;
    if a > b {
        bit ^= 1;
    }
    let result = (x >> 1) | ((bit as u16) << 15);
    (result, bit != original_bit, bit != 0)
}

/// RC-filter coefficient for an exponential approach with the given `tau` (in ticks).
/// `tau <= 0` returns 1.0 (instant). Apply each tick: `current += (target - current) * coeff`.
pub fn rc_coeff(tau: f32) -> f32 {
    if tau <= 0.0 {
        1.0
    } else {
        1.0 - expf(-1.0 / tau)
    }
}

/// Maps a 12-bit fader (0..=4095) to a slide/glide coefficient using an RC approach.
/// Fader 0 → instant (1.0). Fader 4095 → tau ~51 ticks (~150ms settling at 1ms tick).
pub fn fader_to_slide_coeff(fader: u16) -> f32 {
    if fader == 0 {
        1.0
    } else {
        rc_coeff(1.0 + fader as f32 * 50.0 / 4095.0)
    }
}

/// Exponential approach step: moves `current` toward `target` by `coeff`.
pub fn apply_slide(current: f32, target: f32, coeff: f32) -> f32 {
    current + (target - current) * coeff
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

/// Linearly interpolates between two adjacent loop samples at 1 ms resolution.
///
/// `tick_interval_ms`: measured duration of the last clock tick (ms).
/// `ppqn`: the clock division in use. Caps the interpolation window at the
/// interval expected at 20 BPM for that division, so output holds at `next`
/// rather than drifting if the clock stalls.
pub fn interp_loop_sample(
    prev: u16,
    next: u16,
    elapsed_ms: u32,
    tick_interval_ms: u32,
    ppqn: u8,
) -> u16 {
    let max_ms = 60_000 / (20_u32 * ppqn as u32).max(1);
    let interval = tick_interval_ms.clamp(1, max_ms);
    let phase = elapsed_ms.min(interval) as f32 / interval as f32;
    (prev as f32 + (next as f32 - prev as f32) * phase).clamp(0.0, 4095.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slew_state_from_u16_round_trips_through_value() {
        assert_eq!(SlewState::from(0).value(), 0);
        assert_eq!(SlewState::from(2047).value(), 2047);
        assert_eq!(SlewState::from(4095).value(), 4095);
        assert_eq!(SlewState::new().value(), 0);
    }

    #[test]
    fn slew_lin_bypasses_at_minimum_rate() {
        // rate 0 is the minimum: the step size exceeds the bypass threshold,
        // so the very first tick should jump straight to the target.
        let state = slew_lin(SlewState::new(), 4095, 0, 0);
        assert_eq!(state.value(), 4095);
    }

    #[test]
    fn slew_lin_at_max_rate_moves_gradually_without_overshoot() {
        let mut state = SlewState::new();
        let mut prev_val = 0u16;
        // At the slowest rate the Q8 step is ~0.5/tick, so reaching full
        // scale takes ~8190 ticks.
        for _ in 0..8200 {
            state = slew_lin(state, 4095, 4095, 4095);
            let val = state.value();
            // Monotonic, non-decreasing approach toward the target, never overshooting.
            assert!(val >= prev_val);
            assert!(val <= 4095);
            prev_val = val;
        }
        assert_eq!(prev_val, 4095, "should have reached the target eventually");
    }

    #[test]
    fn slew_lin_falls_toward_a_lower_target() {
        let mut state = SlewState::from(4095);
        for _ in 0..8200 {
            state = slew_lin(state, 0, 4095, 4095);
        }
        assert_eq!(state.value(), 0);
    }

    #[test]
    fn slew_exp_converges_to_target_and_stays_in_range() {
        let mut state = SlewState::new();
        for _ in 0..200 {
            state = slew_exp(state, 4095, 3, 3);
            assert!(state.value() <= 4095);
        }
        assert_eq!(state.value(), 4095);
    }

    #[test]
    fn slew_exp_snaps_once_within_derived_threshold() {
        // slew=3 -> derived snap = (3 >> 8) + 1 = 1, so convergence should
        // land exactly on the target rather than asymptotically approach it.
        let mut state = SlewState::from(1000);
        let mut ticks = 0;
        while state.value() != 2000 && ticks < 500 {
            state = slew_exp(state, 2000, 3, 3);
            ticks += 1;
        }
        assert_eq!(state.value(), 2000);
    }

    #[test]
    fn scale_bits_12_7_full_range() {
        assert_eq!(scale_bits_12_7(0).as_int(), 0);
        assert_eq!(scale_bits_12_7(2048).as_int(), 64);
        assert_eq!(scale_bits_12_7(4063).as_int(), 126);
        assert_eq!(scale_bits_12_7(4064).as_int(), 127);
        assert_eq!(scale_bits_12_7(4095).as_int(), 127);
    }

    #[test]
    fn scale_to_12bit_full_window() {
        assert_eq!(scale_to_12bit(0, 16), 0);
        assert_eq!(scale_to_12bit(0xFFFF, 16), 4095);
        assert_eq!(scale_to_12bit(0x8000, 1), 4095);
        assert_eq!(scale_to_12bit(0, 1), 0);
    }

    #[test]
    fn rotate_select_bit_no_flip_loops_with_period_length() {
        // With a > b == false, the register must repeat exactly every `length` steps:
        // the looped bit is the bottom of the top-`length` window.
        for length in 1..=16u16 {
            let start = 0xACE5u16;
            let mut reg = start;
            for _ in 0..length {
                (reg, _, _) = rotate_select_bit(reg, 0, 1, length);
            }
            let mask = if length == 16 {
                0xFFFF
            } else {
                ((1u32 << length) - 1) as u16
            } << (16 - length);
            assert_eq!(reg & mask, start & mask, "length {length} did not loop");
        }
    }

    #[test]
    fn rotate_select_bit_flips_when_a_gt_b() {
        let (_, flipped, _) = rotate_select_bit(0x0000, 1, 0, 16);
        assert!(flipped);
        let (result, _, out_bit) = rotate_select_bit(0x0000, 1, 0, 16);
        assert_eq!(result, 0x8000);
        assert!(out_bit);
    }

    #[test]
    fn interp_midpoint() {
        let out = interp_loop_sample(0, 4000, 50, 100, 1);
        assert!((out as i32 - 2000).abs() < 5);
    }

    #[test]
    fn interp_clamps_past_interval() {
        assert_eq!(interp_loop_sample(0, 3000, 200, 100, 1), 3000);
    }

    #[test]
    fn interp_equal_samples_are_constant() {
        assert_eq!(interp_loop_sample(1000, 1000, 1, 100, 1), 1000);
        assert_eq!(interp_loop_sample(1000, 1000, 50, 100, 1), 1000);
        assert_eq!(interp_loop_sample(1000, 1000, 200, 100, 1), 1000);
    }

    /// Simulates the tick_id-based interpolation state machine.
    /// Returns the maximum single-step output change observed across all 1ms polls.
    fn simulate_max_step(buffer: &[u16], interval_ms: u32, ppqn: u8) -> i32 {
        let mut elapsed_ms: u32 = 0;
        let mut last_tick_id: u8 = 0;
        let mut tick_id: u8 = 0;
        let mut loop_prev: u16 = buffer[0];
        let mut loop_target: u16 = loop_prev;
        let mut prev_out: u16 = buffer[0];
        let mut max_step: i32 = 0;
        for &sample in buffer {
            loop_prev = loop_target;
            loop_target = sample;
            tick_id = tick_id.wrapping_add(1);
            for _ in 0..interval_ms {
                if tick_id != last_tick_id {
                    elapsed_ms = 0;
                    last_tick_id = tick_id;
                }
                elapsed_ms += 1;
                let out = interp_loop_sample(loop_prev, loop_target, elapsed_ms, interval_ms, ppqn);
                let step = (out as i32 - prev_out as i32).abs();
                if step > max_step {
                    max_step = step;
                }
                prev_out = out;
            }
        }
        max_step
    }

    #[test]
    fn no_jump_on_equal_consecutive_samples() {
        // [1000, 1000, 2000]: equal samples followed by movement.
        // At 100ms interval, max step per ms ≈ 10; a snap would be ~1000.
        assert!(simulate_max_step(&[1000, 1000, 2000], 100, 1) < 20);
    }

    #[test]
    fn no_jump_on_normal_movement() {
        assert!(simulate_max_step(&[0, 1000, 2000, 3000], 100, 1) < 20);
    }
}
