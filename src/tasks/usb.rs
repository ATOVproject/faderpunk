use core::sync::atomic::AtomicBool;

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

use portable_atomic::Ordering;
use wmidi::MidiMessage;

use crate::Irqs;

pub enum UsbAction<'a> {
    SendMidiMsg(MidiMessage<'a>),
}

pub static CHANNEL_USB_TX: Channel<CriticalSectionRawMutex, UsbAction, 16> = Channel::new();
pub static USB_CONNECTED: AtomicBool = AtomicBool::new(false);

pub async fn start_usb(spawner: &Spawner, usb0: USB) {
    spawner.spawn(run_usb(usb0)).unwrap();
}

#[derive(Copy, Clone)]
pub enum CodeIndexNumber {
    /// Miscellaneous function codes. Reserved for future extensions.
    MiscFunction = 0x0,
    /// Cable events. Reserved for future expansion.
    // CableEvents = 0x1,
    /// Two-byte System Common messages like MTC, SongSelect, etc.
    SystemCommonLen2 = 0x2,
    /// Three-byte System Common messages like SPP, etc.
    SystemCommonLen3 = 0x3,
    /// SysEx starts or continues.
    SysExStarts = 0x4,
    /// Single-byte System Common Message or SysEx ends with following single byte.
    SystemCommonLen1 = 0x5,
    /// SysEx ends with following two bytes.
    SysExEndsNext2 = 0x6,
    /// SysEx ends with following three bytes.
    SysExEndsNext3 = 0x7,
    /// Note Off
    NoteOff = 0x8,
    /// Note On
    NoteOn = 0x9,
    /// Polyphonic Key Pressure (Aftertouch)
    PolyphonicKeyPressure = 0xA,
    /// Control Change
    ControlChange = 0xB,
    /// Program Change
    ProgramChange = 0xC,
    /// Channel Pressure (Aftertouch)
    ChannelPressure = 0xD,
    /// Pitch Bend Change
    PitchBendChange = 0xE,
    /// Single-byte
    SingleByte = 0xF,
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
            info!("USB Connection established.");
            start_usb_midi_tx_loop(&mut tx).await.ok();
            info!("USB Connection lost?? Starting over.");
        }
    };

    let midi_rx = async {
        loop {
            rx.wait_connection().await;
            USB_CONNECTED.store(true, Ordering::Relaxed);
            start_usb_midi_rx_loop(&mut rx).await.ok();
            USB_CONNECTED.store(false, Ordering::Relaxed);
        }
    };

    join3(usb.run(), midi_tx, midi_rx).await;
}

async fn start_usb_midi_tx_loop<'d, T: usb::Instance + 'd>(
    tx: &mut Sender<'d, usb::Driver<'d, T>>,
) -> Result<(), EndpointError> {
    // FIXME: THIS DOES NOT WORK WITH SYSEX DATA (can be _VERY_ long)
    let mut buf = [0; 4];
    loop {
        if let UsbAction::SendMidiMsg(msg) = CHANNEL_USB_TX.receive().await {
            buf[0] = code_index_number_from_message(&msg) as u8;
            if msg.copy_to_slice(&mut buf[1..msg.bytes_size() + 1]).is_ok() {
                tx.write_packet(&buf[..msg.bytes_size() + 1]).await?;
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

fn code_index_number_from_message(message: &MidiMessage) -> CodeIndexNumber {
    match message {
        MidiMessage::NoteOn(..) => CodeIndexNumber::NoteOn,
        MidiMessage::NoteOff(..) => CodeIndexNumber::NoteOff,
        MidiMessage::PolyphonicKeyPressure(..) => CodeIndexNumber::PolyphonicKeyPressure,
        MidiMessage::ControlChange(..) => CodeIndexNumber::ControlChange,
        MidiMessage::ProgramChange(..) => CodeIndexNumber::ProgramChange,
        MidiMessage::ChannelPressure(..) => CodeIndexNumber::ChannelPressure,
        MidiMessage::MidiTimeCode(..) => CodeIndexNumber::SystemCommonLen2,
        MidiMessage::SongSelect(..) => CodeIndexNumber::SystemCommonLen2,
        MidiMessage::SongPositionPointer(..) => CodeIndexNumber::SystemCommonLen3,
        MidiMessage::TuneRequest => CodeIndexNumber::SystemCommonLen1,
        MidiMessage::PitchBendChange(..) => CodeIndexNumber::PitchBendChange,
        MidiMessage::SysEx(data) => {
            // Determine the appropriate CIN based on the SysEx message length
            match data.len() {
                0 | 1 => CodeIndexNumber::SystemCommonLen1,
                2 => CodeIndexNumber::SysExEndsNext2,
                3 => CodeIndexNumber::SysExEndsNext3,
                _ => CodeIndexNumber::SysExStarts, // Start or continue SysEx
            }
        }
        // All System Real-Time messages are single-byte messages
        MidiMessage::TimingClock
        | MidiMessage::Start
        | MidiMessage::Continue
        | MidiMessage::Stop
        | MidiMessage::ActiveSensing
        | MidiMessage::Reset => CodeIndexNumber::SingleByte,
        _ => CodeIndexNumber::MiscFunction, // Default or unhandled messages
    }
}
