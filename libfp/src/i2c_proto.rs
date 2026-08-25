use serde::{Deserialize, Serialize};

use crate::{
    types::{RegressionValuesInput, RegressionValuesOutput},
    Range, GLOBAL_CHANNELS,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaderUpdate {
    pub channel: usize,
    pub value: u16,
    pub range: Range,
}

impl FaderUpdate {
    pub const fn new(channel: usize, value: u16, range: Range) -> Self {
        Self {
            channel,
            value,
            range,
        }
    }
}

pub struct PendingFaderUpdates {
    updates: [Option<FaderUpdate>; GLOBAL_CHANNELS],
}

impl PendingFaderUpdates {
    pub const fn new() -> Self {
        Self {
            updates: [None; GLOBAL_CHANNELS],
        }
    }

    pub fn publish(&mut self, channel: usize, value: u16, range: Range) {
        if let Some(slot) = self.updates.get_mut(channel) {
            *slot = Some(FaderUpdate::new(channel, value, range));
        }
    }

    pub fn take_all(&mut self) -> [Option<FaderUpdate>; GLOBAL_CHANNELS] {
        core::mem::replace(&mut self.updates, [None; GLOBAL_CHANNELS])
    }
}

impl Default for PendingFaderUpdates {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OutputChangeTracker<const N: usize> {
    previous: [Option<u16>; N],
}

impl<const N: usize> OutputChangeTracker<N> {
    pub const fn new() -> Self {
        Self {
            previous: [None; N],
        }
    }

    pub fn changed(&mut self, values: [u16; N]) -> [Option<u16>; N] {
        core::array::from_fn(|index| {
            if self.previous[index] == Some(values[index]) {
                None
            } else {
                self.previous[index] = Some(values[index]);
                Some(values[index])
            }
        })
    }
}

impl<const N: usize> Default for OutputChangeTracker<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_fader_updates_keep_only_the_latest_value_per_channel() {
        let mut pending = PendingFaderUpdates::new();

        pending.publish(3, 100, Range::_0_10V);
        pending.publish(3, 900, Range::_Neg5_5V);

        let updates = pending.take_all();
        assert_eq!(updates[3], Some(FaderUpdate::new(3, 900, Range::_Neg5_5V)));
        assert!(pending.take_all().iter().all(Option::is_none));
    }

    #[test]
    fn pending_fader_updates_retain_different_channels_independently() {
        let mut pending = PendingFaderUpdates::new();

        pending.publish(2, 200, Range::_0_10V);
        pending.publish(11, 1100, Range::_0_5V);

        let updates = pending.take_all();
        assert_eq!(updates[2], Some(FaderUpdate::new(2, 200, Range::_0_10V)));
        assert_eq!(updates[11], Some(FaderUpdate::new(11, 1100, Range::_0_5V)));
    }

    #[test]
    fn output_change_tracker_reports_both_panner_outputs_and_suppresses_duplicates() {
        let mut tracker = OutputChangeTracker::<2>::new();

        assert_eq!(tracker.changed([1200, 2800]), [Some(1200), Some(2800)]);
        assert_eq!(tracker.changed([1200, 2800]), [None, None]);
        assert_eq!(tracker.changed([1300, 2800]), [Some(1300), None]);
    }
}
