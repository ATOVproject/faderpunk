//! USB transport for the config protocol: packetizes outbound SysEx frames
//! into cable-1 USB-MIDI event packets. The protocol logic itself lives in
//! `fp_core::tasks::configure`.

use embassy_time::{with_timeout, Duration};

use fp_core::tasks::configure::{ConfigSink, ProtocolError};

use crate::tasks::midi::{SharedUsbSender, CONFIG_CABLE};
use crate::tasks::transport::USB_MAX_PACKET_SIZE;
use crate::version::FIRMWARE_VERSION;

/// Per-packet write timeout for config responses. Generous compared to the
/// 1ms performance-MIDI timeout: config frames must not be silently
/// truncated, but a stalled host must not block the USB sender forever.
const CONFIG_WRITE_TIMEOUT_MS: u64 = 500;

pub async fn start_config_loop<'a>(usb_tx: &'a SharedUsbSender<'a>) {
    fp_core::tasks::configure::start_config_loop(UsbConfigSink { usb_tx }, FIRMWARE_VERSION).await
}

/// Writes complete config SysEx frames as cable-1 USB-MIDI event packets,
/// flushed per 64-byte USB packet. The sender mutex is released between USB
/// packets so performance MIDI (cable 0) interleaves during long transfers.
struct UsbConfigSink<'a> {
    usb_tx: &'a SharedUsbSender<'a>,
}

impl ConfigSink for UsbConfigSink<'_> {
    async fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProtocolError> {
        let mut usb_packet = [0u8; USB_MAX_PACKET_SIZE as usize];
        let mut usb_len = 0;
        let total_chunks = frame.len().div_ceil(3);
        let mut last_write_len = 0;
        for (i, chunk) in frame.chunks(3).enumerate() {
            let last = i + 1 == total_chunks;
            let cin: u8 = if last {
                // SysEx ends with following 1/2/3 bytes
                match chunk.len() {
                    1 => 0x5,
                    2 => 0x6,
                    _ => 0x7,
                }
            } else {
                // SysEx starts or continues
                0x4
            };
            usb_packet[usb_len] = (CONFIG_CABLE << 4) | cin;
            usb_packet[usb_len + 1..usb_len + 4].fill(0);
            usb_packet[usb_len + 1..usb_len + 1 + chunk.len()].copy_from_slice(chunk);
            usb_len += 4;
            if usb_len == usb_packet.len() || last {
                write_usb_packet(self.usb_tx, &usb_packet[..usb_len]).await?;
                last_write_len = usb_len;
                usb_len = 0;
            }
        }
        if last_write_len == usb_packet.len() {
            // Terminate the bulk transfer with a ZLP after a full-size packet
            write_usb_packet(self.usb_tx, &[]).await?;
        }

        Ok(())
    }
}

async fn write_usb_packet(usb_tx: &SharedUsbSender<'_>, data: &[u8]) -> Result<(), ProtocolError> {
    let mut tx = usb_tx.lock().await;
    with_timeout(
        Duration::from_millis(CONFIG_WRITE_TIMEOUT_MS),
        tx.write_packet(data),
    )
    .await
    .map_err(|_| ProtocolError::Timeout)?
    .map_err(|_| ProtocolError::TransmissionError)
}
