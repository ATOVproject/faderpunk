//! Shared interface for I2C leader messages (16n-style fader forwarding).
//! The bus driver and follower/calibration logic live in the firmware.

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Sender},
};
use libfp::Range;

pub enum I2cLeaderMessage {
    FaderValue(usize, u16, Range),
}

const I2C_LEADER_CHANNEL_SIZE: usize = 16;

pub static I2C_LEADER_CHANNEL: Channel<
    CriticalSectionRawMutex,
    I2cLeaderMessage,
    I2C_LEADER_CHANNEL_SIZE,
> = Channel::new();
pub type I2cLeaderSender =
    Sender<'static, CriticalSectionRawMutex, I2cLeaderMessage, I2C_LEADER_CHANNEL_SIZE>;
