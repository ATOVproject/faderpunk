use defmt::info;
use embassy_futures::join::{join, join3};
use embassy_rp::peripherals::USB;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_sync::mutex::Mutex;
use embassy_time::{with_timeout, Duration};
use embassy_usb::class::midi::MidiClass;

use embassy_rp::peripherals::{UART0, UART1};
use embassy_rp::uart::{Async, Uart, UartTx};
use embassy_rp::usb::Driver;
// FIXME: Use https://docs.rs/midi2/0.7.0/midi2 instead
use wmidi::MidiMessage;

pub type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, MidiMessage<'static>), 64>;

#[derive(Copy, Clone)]
enum CodeIndexNumber {
    /// Miscellaneous function codes. Reserved for future extensions.
    MiscFunction = 0x0,
    /// Cable events. Reserved for future expansion.
    CableEvents = 0x1,
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

pub async fn start_midi_loops<'a>(
    usb_midi: MidiClass<'a, Driver<'a, USB>>,
    uart0: UartTx<'static, UART0, Async>,
    uart1: Uart<'static, UART1, Async>,
    x_rx: XRxReceiver,
) {
    let (mut usb_tx, mut usb_rx) = usb_midi.split();
    let uart0_tx: Mutex<NoopRawMutex, UartTx<'static, UART0, Async>> = Mutex::new(uart0);
    let (mut uart1_tx, mut uart1_rx) = uart1.split();

    let midi_tx = async {
        let mut buf = [0; 4];
        // TODO: Do not try to send midi message to USB when not connected
        // usb_tx.wait_connection().await;
        loop {
            let (_chan, midi_msg) = x_rx.receive().await;
            buf[0] = cin_from_msg(&midi_msg) as u8;
            if midi_msg
                .copy_to_slice(&mut buf[1..midi_msg.bytes_size() + 1])
                .is_ok()
            {
                // TODO: Handle these Results?
                let _ = join(
                    with_timeout(
                        // 1ms of timeout should be enough for USB host to have acknowledged
                        Duration::from_millis(1),
                        usb_tx.write_packet(&buf[..midi_msg.bytes_size() + 1]),
                    ),
                    uart1_tx.write(&buf[1..midi_msg.bytes_size() + 1]),
                )
                .await;
            }
        }
    };

    let usb_rx = async {
        let mut buf = [0; 64];
        loop {
            if let Ok(len) = usb_rx.read_packet(&mut buf).await {
                info!("LEN: {}", len);
                if len == 0 {
                    continue;
                }
                // Remove USB-Midi CIN
                let data = &buf[1..len];
                // Write to MIDI-THRU
                let mut tx = uart0_tx.lock().await;
                tx.write(data).await.unwrap();
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
    };

    let uart_rx = async {
        let mut buf = [0; 3];
        loop {
            uart1_rx.read(&mut buf).await.unwrap();

            // Write to MIDI-THRU
            let mut tx = uart0_tx.lock().await;
            tx.write(&buf).await.unwrap();
            match MidiMessage::from_bytes(&buf) {
                Ok(_midi_msg) => {
                    info!("DO SOMETHING WITH THIS MESSAGE: {:?}", buf);
                }
                Err(_err) => {
                    info!("There was an error but we should not panic. Data: {}", buf);
                }
            }
        }
    };

    join3(midi_tx, usb_rx, uart_rx).await;
}

fn cin_from_msg(message: &MidiMessage) -> CodeIndexNumber {
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
