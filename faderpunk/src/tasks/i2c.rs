use core::sync::atomic::AtomicBool;

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_rp::i2c::{self, Async};
use embassy_rp::i2c_slave::{self, Command, I2cSlave};
use embassy_rp::peripherals::{I2C0, PIN_20, PIN_21};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex};
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;
use max11300::config::{ConfigMode5, ConfigMode7, Mode, AVR, NSAMPLES};
use mii::{
    devices::{ansible, er301, telexo},
    Command as MiiCommand,
};
use portable_atomic::Ordering;

use libfp::{
    i2c_proto::{
        DeviceStatus, ErrorCode, Response, WriteCommand, WriteReadCommand, MAX_MESSAGE_SIZE,
    },
    types::{RegressionValuesInput, RegressionValuesOutput},
    I2cMode, I2C_ADDRESS_CALIBRATION,
};
use postcard::{from_bytes, to_slice};

use crate::tasks::calibration::run_calibration;
use crate::tasks::global_config::get_global_config;
use crate::tasks::max::{MaxCmd, MAX_CHANNEL, MAX_VALUES_ADC};
use crate::Irqs;

use super::max::MAX_VALUES_DAC;

pub type I2cDevice = I2cSlave<'static, I2C0>;

#[allow(clippy::large_enum_variant)]
pub enum I2cFollowerMessage {
    CalibStart,
    CalibSetRegressionValues(RegressionValuesInput, RegressionValuesOutput),
}

pub enum I2cLeaderMessage {
    FaderValue(usize, u16),
}

const I2C_LEADER_CHANNEL_SIZE: usize = 16;
const I2C_FOLLOWER_CHANNEL_SIZE: usize = 8;

pub static I2C_CONNECTED: AtomicBool = AtomicBool::new(false);

pub static I2C_LEADER_CHANNEL: Channel<
    CriticalSectionRawMutex,
    I2cLeaderMessage,
    I2C_LEADER_CHANNEL_SIZE,
> = Channel::new();
pub type I2cLeaderSender =
    Sender<'static, CriticalSectionRawMutex, I2cLeaderMessage, I2C_LEADER_CHANNEL_SIZE>;

pub static I2C_FOLLOWER_CHANNEL: Channel<
    ThreadModeRawMutex,
    I2cFollowerMessage,
    I2C_FOLLOWER_CHANNEL_SIZE,
> = Channel::new();
pub type I2cFollowerReceiver =
    Receiver<'static, ThreadModeRawMutex, I2cFollowerMessage, I2C_FOLLOWER_CHANNEL_SIZE>;
pub type I2cFollowerSender =
    Sender<'static, ThreadModeRawMutex, I2cFollowerMessage, I2C_FOLLOWER_CHANNEL_SIZE>;

pub async fn start_i2c(
    spawner: &Spawner,
    i2c0: Peri<'static, I2C0>,
    scl: Peri<'static, PIN_21>,
    sda: Peri<'static, PIN_20>,
) {
    let global_config = get_global_config();
    match global_config.i2c_mode {
        I2cMode::Calibration => {
            let msg_receiver = I2C_FOLLOWER_CHANNEL.receiver();
            let msg_sender = I2C_FOLLOWER_CHANNEL.sender();
            let mut i2c0_config = i2c_slave::Config::default();
            i2c0_config.addr = I2C_ADDRESS_CALIBRATION;
            let i2c_device = i2c_slave::I2cSlave::new(i2c0, scl, sda, Irqs, i2c0_config);
            spawner
                .spawn(run_i2c_follower(i2c_device, msg_sender, true))
                .unwrap();
            run_calibration(msg_receiver).await;
        }
        I2cMode::Follower => {
            // Currently unimplemented
            // let msg_receiver = I2C_FOLLOWER_CHANNEL.receiver();
            // let msg_sender = I2C_FOLLOWER_CHANNEL.sender();
            // let mut i2c0_config = i2c_slave::Config::default();
            // i2c0_config.addr = I2C_ADDRESS;
            // let i2c_device = i2c_slave::I2cSlave::new(i2c0, scl, sda, Irqs, i2c0_config);
            // spawner
            //     .spawn(run_i2c_follower(i2c_device, msg_sender, false))
            //     .unwrap();
        }
        I2cMode::Leader => {
            let mut i2c0_config = i2c::Config::default();
            i2c0_config.frequency = 400_000;
            let i2c0 = i2c::I2c::new_async(i2c0, scl, sda, Irqs, i2c0_config);
            spawner.spawn(run_i2c_leader(i2c0)).unwrap();
        }
    }
}

#[derive(Default, Clone, Copy)]
struct DiscoveredDevices {
    ansible: bool,
    er301: bool,
    txo: bool,
}

struct Compat16N<'a> {
    i2c: &'a mut i2c::I2c<'static, I2C0, Async>,
    devices: DiscoveredDevices,
    buffer: [u8; 8],
    errors: usize,
}

impl<'a> Compat16N<'a> {
    async fn new(i2c: &'a mut i2c::I2c<'static, I2C0, Async>) -> Self {
        // Scan for i2c devices
        let mut devices = DiscoveredDevices::default();

        for addr in 8..=120 {
            if i2c.write(addr, &[]).await.is_ok() {
                match addr {
                    ansible::ADDRESS => {
                        devices.ansible = true;
                        I2C_CONNECTED.store(true, Ordering::Relaxed);
                    }
                    er301::ADDRESS => {
                        devices.er301 = true;
                        I2C_CONNECTED.store(true, Ordering::Relaxed);
                    }
                    a if (telexo::BASE_ADDRESS..telexo::BASE_ADDRESS + 8).contains(&a) => {
                        devices.txo = true;
                        I2C_CONNECTED.store(true, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        }

        Self {
            i2c,
            devices,
            buffer: [0u8; 8],
            errors: 0,
        }
    }

    async fn handle_fader_update(&mut self, chan: usize, value: u16) {
        // Send to ER-301 if present
        if self.devices.er301 {
            let cmd = er301::Commands::SetCv {
                port: chan as u8,
                value: value as i16,
            };
            if let Ok(msg) = cmd.to_bytes(&mut self.buffer) {
                if self.i2c.write(er301::ADDRESS, msg).await.is_err() {
                    error!("I2C write to ER-301 failed");
                    self.errors += 1;
                }
            }
        }

        // Send to TXo if present
        if self.devices.txo {
            let device_index = (chan / 4) as u8;
            let port = (chan % 4) as u8;
            let address = telexo::BASE_ADDRESS + device_index;
            let cmd = telexo::Commands::SetCv {
                port,
                value: value as i16,
            };

            if let Ok(msg) = cmd.to_bytes(&mut self.buffer) {
                if self.i2c.write(address, msg).await.is_err() {
                    error!("I2C write to TXo (addr 0x{:02X}) failed", address);
                    self.errors += 1;
                }
            }
        }

        // Send to Ansible if present
        if self.devices.ansible {
            let device_port = ((chan / 4) << 1) as u8;
            let cmd = ansible::Commands::SetCvFromFader { device_port, value };

            if let Ok(msg) = cmd.to_bytes(&mut self.buffer) {
                if self.i2c.write(ansible::ADDRESS, msg).await.is_err() {
                    error!("I2C write to Ansible failed");
                    self.errors += 1;
                }
            }
        }

        // If we accumulate a lot of errors, we assume that the i2c device was removed.
        // Disable sending messages
        if self.errors >= I2C_LEADER_CHANNEL_SIZE {
            I2C_CONNECTED.store(false, Ordering::Relaxed);
        }
    }
}

async fn process_write_read(command: WriteReadCommand) -> Response {
    match command {
        WriteReadCommand::AdcGetVoltage(channel, range) => {
            MAX_CHANNEL
                .send((
                    channel,
                    MaxCmd::ConfigurePort(
                        Mode::Mode7(ConfigMode7(
                            AVR::InternalRef,
                            range.into(),
                            NSAMPLES::Samples1,
                        )),
                        None,
                    ),
                ))
                .await;
            Timer::after_millis(100).await;
            let value = MAX_VALUES_ADC[channel].load(Ordering::Relaxed);
            Response::AdcValue(channel, range, value)
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

async fn process_write(command: WriteCommand, sender: &mut I2cFollowerSender) {
    match command {
        WriteCommand::CalibStart => {
            // Send command to i2c follower channel
            sender.send(I2cFollowerMessage::CalibStart).await;
        }
        WriteCommand::CalibSetRegValues(input_values, output_values) => {
            sender
                .send(I2cFollowerMessage::CalibSetRegressionValues(
                    input_values,
                    output_values,
                ))
                .await;
        }
        WriteCommand::DacSetVoltage(channel, range, value) => {
            MAX_CHANNEL
                .send((
                    channel,
                    MaxCmd::ConfigurePort(Mode::Mode5(ConfigMode5(range.into())), None),
                ))
                .await;
            MAX_VALUES_DAC[channel].store(value, Ordering::Relaxed);
        }
        WriteCommand::SysReset => {
            cortex_m::peripheral::SCB::sys_reset();
        }
    }
}

#[embassy_executor::task]
async fn run_i2c_follower(
    mut i2c_device: I2cDevice,
    mut msg_sender: I2cFollowerSender,
    // TODO: use this to disable calibration i2c commands?
    _calibrating: bool,
) {
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
                    Ok(command) => process_write(command, &mut msg_sender).await,
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

#[embassy_executor::task]
async fn run_i2c_leader(mut i2c: i2c::I2c<'static, I2C0, Async>) {
    // Wait for followers to boot
    Timer::after_secs(10).await;

    let mut compat16n = Compat16N::new(&mut i2c).await;

    loop {
        let I2cLeaderMessage::FaderValue(chan, value) = I2C_LEADER_CHANNEL.receive().await;
        compat16n.handle_fader_update(chan, value).await;
    }
}
