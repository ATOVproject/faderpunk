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
use midi2::channel_voice1::ChannelVoice1;
use midi2::Data;

pub type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, ChannelVoice1<[u8; 3]>), 64>;

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
    KeyPressure = 0xA,
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
        // TODO: Deal with backpressure as well (do it on core b maybe?)
        // See https://claude.ai/chat/1a702bdf-b1f9-4d52-a004-aa221cbb4642 for improving this
        loop {
            let (_chan, midi_msg) = x_rx.receive().await;
            buf[0] = cin_from_bytes_msg(&midi_msg) as u8;
            buf[1..].copy_from_slice(midi_msg.data());
            // TODO: Handle these Results?
            let _ = join(
                with_timeout(
                    // 1ms of timeout should be enough for USB host to have acknowledged
                    Duration::from_millis(1),
                    // Write including USB-MIDI CIN
                    usb_tx.write_packet(&buf),
                ),
                // Write excluding USB-MIDI CIN
                uart1_tx.write(&buf[1..]),
            )
            .await;
        }
    };

    let usb_rx = async {
        let mut buf = [0; 64];
        loop {
            if let Ok(len) = usb_rx.read_packet(&mut buf).await {
                if len == 0 {
                    continue;
                }
                // Remove USB-MIDI CIN
                let data = &buf[1..len];
                // Write to MIDI-THRU
                let mut tx = uart0_tx.lock().await;
                tx.write(data).await.unwrap();
                match ChannelVoice1::try_from(data) {
                    Ok(_midi_msg) => {
                        // TODO: DO SOMETHING WITH THIS MESSAGE
                    }
                    Err(_err) => {
                        // TODO: Log with USB
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
            if let Err(err) = uart1_rx.read(&mut buf).await {
                info!("uart rx err: {}", err);
                continue;
            }

            // Write to MIDI-THRU
            let mut tx = uart0_tx.lock().await;
            tx.write(&buf).await.unwrap();

            match ChannelVoice1::try_from(buf.as_slice()) {
                Ok(_midi_msg) => {
                    // TODO: DO SOMETHING WITH THIS MESSAGE
                }
                Err(_err) => {
                    // TODO: Log with USB
                    info!("There was an error but we should not panic. Data: {}", buf);
                }
            }
        }
    };

    join3(midi_tx, usb_rx, uart_rx).await;
}

fn cin_from_bytes_msg(msg: &ChannelVoice1<[u8; 3]>) -> CodeIndexNumber {
    match msg {
        ChannelVoice1::NoteOn(..) => CodeIndexNumber::NoteOn,
        ChannelVoice1::NoteOff(..) => CodeIndexNumber::NoteOff,
        ChannelVoice1::KeyPressure(..) => CodeIndexNumber::KeyPressure,
        ChannelVoice1::ChannelPressure(..) => CodeIndexNumber::ChannelPressure,
        ChannelVoice1::ProgramChange(..) => CodeIndexNumber::ProgramChange,
        ChannelVoice1::ControlChange(..) => CodeIndexNumber::ControlChange,
        ChannelVoice1::PitchBend(..) => CodeIndexNumber::PitchBendChange,
    }
}
