use defmt::Format;
use serde::{Deserialize, Serialize};

/// Maximum size of a serialized message in bytes.
/// This must be large enough for the largest possible message.
pub const MAX_MESSAGE_SIZE: usize = 64;

/// Commands sent from the calibrator to the device.
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum Command {
    /// Request a measurement from a specific channel.
    ReadChannel(u8),
    /// Get the device's current status.
    GetStatus,
}

/// Responses sent from the device to the calibrator.
#[derive(Serialize, Deserialize, Debug, PartialEq, Format)]
pub enum Response {
    /// The value read from a channel.
    ChannelValue(f32),
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
