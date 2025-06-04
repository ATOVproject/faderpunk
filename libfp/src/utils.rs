use embassy_time::Duration;
use midly::num::u7;

pub const fn bpm_to_clock_duration(bpm: f64, ppqn: u8) -> Duration {
    Duration::from_nanos((1_000_000_000.0 / (bpm / 60.0 * ppqn as f64)) as u64)
}

/// Scale from 4096 to 127
pub fn scale_bits_12_7(value: u16) -> u7 {
    u7::new(((value as u32 * 127) / 4095) as u8)
}
