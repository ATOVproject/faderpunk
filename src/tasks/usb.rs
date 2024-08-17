use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::midi::{MidiClass, Receiver, Sender};
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config};

use wmidi::MidiMessage;

use crate::Irqs;

pub enum UsbAction<'a> {
    SendMidiMsg(MidiMessage<'a>),
}

pub static CHANNEL_USB_TX: Channel<CriticalSectionRawMutex, UsbAction, 16> = Channel::new();

pub async fn start_usb(spawner: &Spawner, usb0: USB) {
    spawner.spawn(run_usb(usb0)).unwrap();
}

#[embassy_executor::task]
async fn run_usb(usb0: USB) {
    let usb_driver = usb::Driver::new(usb0, Irqs);
    let mut usb_config = Config::new(0xc0de, 0xcafe);
    usb_config.manufacturer = Some("ATOV");
    usb_config.product = Some("Phoenix16");
    usb_config.serial_number = Some("12345678");
    usb_config.max_power = 500;
    // usb_config.max_packet_size_0 = 64;
    usb_config.device_class = 0xEF;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;
    usb_config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut usb_builder = Builder::new(
        usb_driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // Create classes on the builder.
    let usb_midi = MidiClass::new(&mut usb_builder, 1, 1, 64);

    let mut usb = usb_builder.build();

    let (mut tx, mut rx) = usb_midi.split();

    // FIXME: Maybe we can move the midi stuff into an own file?
    let midi_tx = async {
        loop {
            // This loop automatically reconnects to the device when it is disconnected.
            tx.wait_connection().await;
            start_usb_midi_tx_loop(&mut tx).await.ok();
        }
    };

    let midi_rx = async {
        loop {
            rx.wait_connection().await;
            start_usb_midi_rx_loop(&mut rx).await.ok();
        }
    };

    join3(usb.run(), midi_tx, midi_rx).await;
}

async fn start_usb_midi_tx_loop<'d, T: usb::Instance + 'd>(
    tx: &mut Sender<'d, usb::Driver<'d, T>>,
) -> Result<(), EndpointError> {
    // FIXME: THIS DOES NOT WORK WITH SYSEX DATA (can be _VERY_ long)
    let mut buf = [0; 64];
    loop {
        if let UsbAction::SendMidiMsg(msg) = CHANNEL_USB_TX.receive().await {
            if msg.copy_to_slice(&mut buf[..msg.bytes_size()]).is_ok() {
                tx.write_packet(&buf[..msg.bytes_size()]).await?
            }
        }
    }
}

async fn start_usb_midi_rx_loop<'d, T: usb::Instance + 'd>(
    rx: &mut Receiver<'d, usb::Driver<'d, T>>,
) -> Result<(), EndpointError> {
    let mut buf = [0; 64];
    loop {
        if let Ok(len) = rx.read_packet(&mut buf).await {
            // Remove USB-Midi CIN
            let data = &buf[1..len];
            match MidiMessage::from_bytes(data) {
                Ok(_midi_msg) => {
                    info!("DO SOMETHING WITH THIS MESSAGE: {:?}", data);
                }
                Err(_err) => {
                    info!(
                        "There was an error but we should not panic. Len: {}, Data: {}",
                        len, data
                    );
                }
            }
        }
    }
}
