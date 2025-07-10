use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_rp::i2c_slave::{Command, I2cSlave};
use embassy_rp::peripherals::I2C0;

use libfp::i2c_proto::{
    Command as ProtoCommand, DeviceStatus, ErrorCode, Response, MAX_MESSAGE_SIZE,
};
use postcard::{from_bytes, to_slice};

type I2cDevice = I2cSlave<'static, I2C0>;

pub async fn start_i2c(spawner: &Spawner, i2c_device: I2cDevice) {
    spawner.spawn(run_i2c(i2c_device)).unwrap();
}
// This function contains the logic to process commands
fn process_command(command: ProtoCommand) -> Response {
    match command {
        ProtoCommand::ReadChannel(ch) => {
            info!("Processing command: ReadChannel({})", ch);
            // TODO: Implement actual measurement logic here
            // For now, returning a dummy value for demonstration
            Response::ChannelValue(ch as f32 * 1.23)
        }
        ProtoCommand::GetStatus => {
            info!("Processing command: GetStatus");
            // TODO: Return the actual device status
            Response::Status(DeviceStatus::Idle)
        }
    }
}

#[embassy_executor::task]
async fn run_i2c(mut i2c_device: I2cDevice) {
    let mut buf = [0u8; MAX_MESSAGE_SIZE];

    loop {
        match i2c_device.listen(&mut buf).await {
            // This is the main path for atomic `write_read` calls from the master
            Ok(Command::WriteRead(len)) => {
                info!("Device received WriteRead: {:x}", &buf[..len]);

                // 1. Attempt to deserialize the master's command
                let response = match from_bytes::<ProtoCommand>(&buf[..len]) {
                    Ok(command) => process_command(command),
                    Err(_) => {
                        error!("Failed to deserialize command from master");
                        Response::Error(ErrorCode::InvalidCommand)
                    }
                };

                // 2. Serialize the response to send back
                let mut response_buf = [0u8; MAX_MESSAGE_SIZE];
                match to_slice(&response, &mut response_buf) {
                    Ok(serialized_response) => {
                        // 3. Send the serialized response back to the master
                        match i2c_device.respond_and_fill(serialized_response, 0x00).await {
                            Ok(status) => info!("Responded successfully: {}", status),
                            Err(e) => error!("Error while responding: {}", e),
                        }
                    }
                    Err(_) => {
                        error!("Failed to serialize response");
                        // If we can't serialize, we can't send an error response.
                        // The master will likely time out.
                    }
                }
            }

            Ok(Command::Write(len)) => {
                info!("Device received a Write-Only command: {}", &buf[..len]);
                // This pattern is not currently used by the master.
            }
            Ok(Command::Read) => {
                info!("Device received a Read-Only command.");
                // This pattern is not currently used by the master.
            }
            Ok(Command::GeneralCall(len)) => {
                info!("Device received a General Call: {}", &buf[..len]);
            }

            Err(e) => error!("I2C listen error: {}", e),
        }
    }
}
