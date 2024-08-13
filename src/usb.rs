use embassy_futures::join::join;
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_usb::class::midi::MidiClass;
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config};

// let usb_driver = usb::Driver::new(p.USB, Irqs);
// let mut usb_config = Config::new(0xc0de, 0xcafe);
// usb_config.manufacturer = Some("ATOV");
// usb_config.product = Some("Phoenix16");
// usb_config.serial_number = Some("12345678");
// usb_config.max_power = 500;
// // usb_config.max_packet_size_0 = 64;
// usb_config.device_class = 0xEF;
// usb_config.device_sub_class = 0x02;
// usb_config.device_protocol = 0x01;
// usb_config.composite_with_iads = true;
//
// // Create embassy-usb DeviceBuilder using the driver and config.
// // It needs some buffers for building the descriptors.
// let mut config_descriptor = [0; 256];
// let mut bos_descriptor = [0; 256];
// let mut control_buf = [0; 64];
//
// let mut usb_builder = Builder::new(
//     usb_driver,
//     usb_config,
//     &mut config_descriptor,
//     &mut bos_descriptor,
//     &mut [], // no msos descriptors
//     &mut control_buf,
// );
//
// // Create classes on the builder.
// let mut usb_midi = MidiClass::new(&mut usb_builder, 1, 1, 64);
//
// let mut usb = usb_builder.build();
// let usb_fut = usb.run();
//
// let midi_fut = async {
//     loop {
//         usb_midi.wait_connection().await;
//         info!("Connected");
//         let _ = midi_echo(&mut usb_midi).await;
//         info!("Disconnected");
//     }
// };
//
// join(usb_fut, midi_fut).await;
//

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn midi_echo<'d, T: usb::Instance + 'd>(
    class: &mut MidiClass<'d, usb::Driver<'d, T>>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("data: {:x}", data);
        class.write_packet(data).await?;
    }
}
