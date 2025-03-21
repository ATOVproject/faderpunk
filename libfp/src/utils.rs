use ux::u7;

pub const fn bpm_to_ms(bpm: f32) -> u64 {
    (60000.0 / bpm) as u64
}

pub const fn ms_to_bpm(ms: u64) -> f32 {
    60000.0 / ms as f32
}

/// Scale from 4096 to 127
pub fn u16_to_u7(value: u16) -> u7 {
    u7::new(((value as u32 * 127) / 4095) as u8)
}
