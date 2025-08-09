use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_rp::i2c_slave::{Command, I2cSlave};
use embassy_rp::peripherals::I2C0;
use max11300::config::{ConfigMode5, DACRANGE};
use portable_atomic::Ordering;

use libfp::i2c_proto::{
    DeviceStatus, ErrorCode, Response, WriteCommand, WriteReadCommand, MAX_MESSAGE_SIZE,
};
use postcard::{from_bytes, to_slice};

use crate::storage::store_calibration_data;
use crate::tasks::max::{MaxCalibration, MaxCmd, MaxConfig, MAX_CHANNEL};

use super::max::MAX_VALUES_DAC;

type I2cDevice = I2cSlave<'static, I2C0>;

pub async fn start_i2c(spawner: &Spawner, i2c_device: I2cDevice) {
    spawner.spawn(run_i2c(i2c_device)).unwrap();
}

async fn process_write_read(command: WriteReadCommand) -> Response {
    match command {
        WriteReadCommand::CalibSetVoltage(channel, bipolar_range, value) => {
            info!(
                "Setting voltage value to {} on channel {}. -5 to 5V range: {}",
                value, channel, bipolar_range
            );
            let range = if bipolar_range {
                DACRANGE::RgNeg5_5v
            } else {
                DACRANGE::Rg0_10v
            };
            MAX_CHANNEL
                .send((
                    channel,
                    MaxCmd::ConfigurePort(MaxConfig::Mode5(ConfigMode5(range))),
                ))
                .await;
            MAX_VALUES_DAC[channel].store(value, Ordering::Relaxed);
            Response::CalibVoltageSet(channel)
        }
        WriteReadCommand::CalibSetRegressionValues(values) => {
            let data = MaxCalibration {
                outputs: values,
                ..Default::default()
            };
            store_calibration_data(&data).await;
            Response::Ack
        }
        WriteReadCommand::SysReset => {
            cortex_m::peripheral::SCB::sys_reset();
        }
        WriteReadCommand::GetStatus => {
            // TODO: Return the actual device status
            Response::Status(DeviceStatus::Idle)
        }
    }
}

async fn process_write(command: WriteCommand) {
    match command {
        WriteCommand::SysReset => {
            cortex_m::peripheral::SCB::sys_reset();
        }
    }
}

#[embassy_executor::task]
async fn run_i2c(mut _i2c_device: I2cDevice) {
    // Run i2c business here
}

pub async fn run_calibration(i2c_device: &mut I2cDevice) {
    let mut buf = [0u8; MAX_MESSAGE_SIZE];

    loop {
        match i2c_device.listen(&mut buf).await {
            Ok(Command::WriteRead(len)) => {
                let response = match from_bytes::<WriteReadCommand>(&buf[..len]) {
                    Ok(command) => process_write_read(command).await,
                    Err(_) => {
                        error!("Failed to deserialize write_read command from master");
                        Response::Error(ErrorCode::InvalidCommand)
                    }
                };

                let mut response_buf = [0u8; MAX_MESSAGE_SIZE];
                match to_slice(&response, &mut response_buf) {
                    Ok(serialized_response) => {
                        if i2c_device
                            .respond_and_fill(serialized_response, 0x00)
                            .await
                            .is_err()
                        {
                            error!("Error while responding");
                        }
                    }
                    Err(_) => {
                        error!("Failed to serialize response");
                    }
                }
            }

            Ok(Command::Write(len)) => {
                match from_bytes::<WriteCommand>(&buf[..len]) {
                    Ok(command) => process_write(command).await,
                    Err(_) => {
                        error!("Failed to deserialize write command from master");
                    }
                };
            }
            Ok(Command::Read) => {
                // This is just for showing up on i2c scanners
                if i2c_device.respond_to_read(&[0x00]).await.is_err() {
                    error!("Failed to respond to I2C read request");
                }
            }
            Ok(Command::GeneralCall(len)) => {
                info!("Device received a General Call: {}", &buf[..len]);
            }

            Err(e) => error!("I2C listen error: {}", e),
        }
    }
}
