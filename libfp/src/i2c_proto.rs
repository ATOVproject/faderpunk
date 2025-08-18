use defmt::Format;
use serde::{Deserialize, Serialize};

use crate::types::RegressionValuesOutput;

/// Maximum size of a serialized message in bytes.
/// This must be large enough for the largest possible message.
pub const MAX_MESSAGE_SIZE: usize = 192;

/// WriteReadCommands sent from the i2c leader to the device
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum WriteReadCommand {
    /// Ask for the current calibration port
    CalibPollPort,
    /// Request to set an output port to a certain value
    /// (channel, bipolar_range, value)
    DacSetVoltage(usize, bool, u16),
    /// Set the calculated regression values for output voltages
    CalibSetRegressionValues(RegressionValuesOutput),
    /// Get the device's current status.
    GetStatus,
    /// Reset the device
    SysReset,
}

/// WriteCommands sent from the leader to the device
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum WriteCommand {
    /// Start automatic calibration
    CalibStart,
    /// Calibration: plug in this port
    CalibPlugInPort(usize),
    /// Reset the device
    SysReset,
}

/// Responses sent from the device to the leader
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum Response {
    /// Tell the calibrator that we set the value
    CalibVoltageSet(usize),
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
}

/// Represents the status of the device.
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum DeviceStatus {
    Idle,
    Measuring,
    Error,
}

/// Represents possible error codes.
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum ErrorCode {
    InvalidCommand,
    InvalidChannel,
    MeasurementFailed,
}
