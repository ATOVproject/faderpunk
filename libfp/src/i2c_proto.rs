use serde::{Deserialize, Serialize};

use crate::{
    types::{RegressionValuesInput, RegressionValuesOutput},
    Range,
};

/// Maximum size of a serialized message in bytes.
/// This must be large enough for the largest possible message.
pub const MAX_MESSAGE_SIZE: usize = 384;

/// WriteReadCommands sent from the i2c leader to the device
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum WriteReadCommand {
    /// (channel, range)
    AdcGetVoltage(usize, Range),
    /// Get the device's current status.
    GetStatus,
    /// Reset the device
    SysReset,
}

/// WriteCommands sent from the leader to the device
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum WriteCommand {
    /// Start automatic calibration
    CalibStart,
    /// Set the calculated regression values
    CalibSetRegValues(RegressionValuesInput, RegressionValuesOutput),
    /// (channel, range, value)
    DacSetVoltage(usize, Range, u16),
    /// Reset the device
    SysReset,
}

/// Responses sent from the device to the leader
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Response {
    /// The current status of the device.
    Status(DeviceStatus),
    /// Acknowledgment of a command that doesn't return data.
    Ack,
    /// An error occurred.
    Error(ErrorCode),
    /// ADC Value of an ADC channel (channel, range, value)
    AdcValue(usize, Range, u16),
}

/// Represents the status of the device.
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum DeviceStatus {
    Idle,
    Measuring,
    Error,
}

/// Represents possible error codes.
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum ErrorCode {
    InvalidCommand,
    InvalidChannel,
    MeasurementFailed,
}
