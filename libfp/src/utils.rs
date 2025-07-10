use embassy_time::Duration;
use midly::num::u7;

pub const fn bpm_to_clock_duration(bpm: f64, ppqn: u8) -> Duration {
    Duration::from_nanos((1_000_000_000.0 / (bpm / 60.0 * ppqn as f64)) as u64)
}

/// Scale from 4096 to 127
pub fn scale_bits_12_7(value: u16) -> u7 {
    u7::new(((value as u32 * 127) / 4095) as u8)
}

///return bool of values are close
pub fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}

///split 0 to 4095 value to two 0-255 u8 used for LEDs
pub fn split_unsigned_value(input: u16) -> [u8; 2] {
    let clamped = input.min(4095);
    if clamped <= 2047 {
        let neg = ((2047 - clamped)/8 ).min(255) as u8;
        [0, neg]
    } else {
        let pos = ((clamped - 2047)/8).min(255) as u8;
        [pos, 0]
    }
}

///split -2047 2047 value to two 0-255 u8 used for LEDs
pub fn split_signed_value(input: i32) -> [u8; 2] {
    let clamped = input.clamp(-2047, 2047);
    if clamped >= 0 {
        let pos = ((clamped * 255 + 1023) / 2047) as u8;
        [pos, 0]
    } else {
        let neg = (((-clamped) * 255 + 1023) / 2047) as u8;
        [0, neg]
    }
}

///attenuate a u12 by another u12
pub fn attenuate(signal: u16, level: u16) -> u16 {

    let attenuated = (signal as u32 * level as u32) / 4095;

    attenuated as u16
}

///use to attenuate 0-4095 representing a bipolar value
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