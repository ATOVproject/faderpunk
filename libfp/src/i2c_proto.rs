use serde::{Deserialize, Serialize};

use crate::{types::RegressionValuesOutput, Range};

/// Maximum size of a serialized message in bytes.
/// This must be large enough for the largest possible message.
pub const MAX_MESSAGE_SIZE: usize = 192;

/// WriteReadCommands sent from the i2c leader to the device
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum WriteReadCommand {
    /// Ask for the current calibration port
    CalibPollPort,
    /// (channel, range, value)
    DacSetVoltage(usize, Range, u16),
    /// Get the device's current status.
    GetStatus,
    /// Reset the device
    SysReset,
}

/// WriteCommands sent from the leader to the device
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum WriteCommand {
    /// Start automatic calibration
    CalibStart,
    /// Calibration: plug in this port
    CalibPlugInPort(usize),
    /// Set the calculated regression values for output voltages
    CalibSetRegOutValues(RegressionValuesOutput),
    /// Reset the device
    SysReset,
}

/// Responses sent from the device to the leader
#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Response {
    /// The current calibration port
    CalibPort(usize),
    /// Respond with the current calibration porr
    CalibCurrentPort(usize),
    /// The current status of the device.
    Status(DeviceStatus),
    /// Acknowledgment of a command that doesn't return data.
    Ack,
    /// An error occurred.
    Error(ErrorCode),
    /// Acknowledge that we set the voltage for channel
    CalibVoltageSet(usize),
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
