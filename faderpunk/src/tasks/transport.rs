use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_usb::class::midi::MidiClass;
use embassy_usb::{Builder, Config as UsbConfig};

use embassy_rp::uart::{Async, BufferedUart, UartTx};

use crate::tasks::configure::start_config_loop;
use crate::tasks::midi::{midi_in_task, midi_out_task};
use crate::version::USB_RELEASE_VERSION;

const USB_VENDOR_ID: u16 = 0xf569;
const USB_PRODUCT_ID: u16 = 0x1;
const USB_VENDOR_NAME: &str = "ATOV";
const USB_PRODUCT_NAME: &str = "Faderpunk";

pub const USB_MAX_PACKET_SIZE: u16 = 64;

pub async fn start_transports(
    spawner: &Spawner,
    usb_driver: usb::Driver<'static, USB>,
    uart0: UartTx<'static, Async>,
    uart1: BufferedUart,
    chip_id: u64,
) {
    spawner
        .spawn(run_transports(usb_driver, uart0, uart1, chip_id))
        .unwrap();
}

#[embassy_executor::task]
async fn run_transports(
    usb_driver: usb::Driver<'static, USB>,
    uart0_tx: UartTx<'static, Async>,
    uart1: BufferedUart,
    chip_id: u64,
) {
    // Convert chip ID to hex string for USB serial number
    let mut serial_buf = [0u8; 16];
    let chip_id_bytes = chip_id.to_be_bytes();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for (i, &byte) in chip_id_bytes.iter().enumerate() {
        serial_buf[i * 2] = HEX[(byte >> 4) as usize];
        serial_buf[i * 2 + 1] = HEX[(byte & 0x0F) as usize];
    }
    // Safety: We just filled the buffer with valid ASCII hex chars
    let serial_number = unsafe { core::str::from_utf8_unchecked(&serial_buf) };

    let mut usb_config = UsbConfig::new(USB_VENDOR_ID, USB_PRODUCT_ID);
    usb_config.manufacturer = Some(USB_VENDOR_NAME);
    usb_config.product = Some(USB_PRODUCT_NAME);
    usb_config.serial_number = Some(serial_number);
    usb_config.device_release = USB_RELEASE_VERSION;
    usb_config.max_power = 500;
    usb_config.max_packet_size_0 = USB_MAX_PACKET_SIZE as u8;
    // Deliberately a pure single-function MIDI-class device: bDeviceClass 0x00,
    // no IADs, no vendor interfaces, no MS OS descriptors. Embedded USB MIDI
    // hosts refuse to exchange MIDI with devices carrying any non-MIDI
    // interface (see docs/usb-host-compatibility.md), which is why the
    // configurator protocol runs as SysEx on the second virtual MIDI cable.

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 128];
    let mut control_buf = [0; 64];

    let mut usb_builder = Builder::new(
        usb_driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no MS OS descriptors
        &mut control_buf,
    );

    // Two virtual cables: cable 0 = performance MIDI, cable 1 = config SysEx
    let usb_midi = MidiClass::new(&mut usb_builder, 2, 2, USB_MAX_PACKET_SIZE);

    let (usb_tx, usb_rx) = usb_midi.split();
    // Shared between performance MIDI out and the config loop. All consumers
    // are futures joined in this task, so a NoopRawMutex suffices. Declared
    // before `usb` so it outlives the device's drop.
    let usb_tx = Mutex::<NoopRawMutex, _>::new(usb_tx);
    let (uart1_tx, uart1_rx) = uart1.split();

    let mut usb = usb_builder.build();

    let midi_out_fut = midi_out_task(&usb_tx, uart0_tx, uart1_tx);
    let midi_in_fut = midi_in_task(usb_rx, uart1_rx);
    let config_fut = start_config_loop(&usb_tx);

    join4(usb.run(), midi_in_fut, midi_out_fut, config_fut).await;
}
