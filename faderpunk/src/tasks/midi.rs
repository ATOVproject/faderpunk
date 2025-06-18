use config::{ClockSrc, GlobalConfig};
use defmt::info;
use embassy_futures::join::{join, join4};
use embassy_rp::peripherals::USB;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::mutex::Mutex;
use embassy_time::{with_timeout, Duration, TimeoutError};
use embassy_usb::class::midi::{MidiClass, Sender as UsbSender};
use embedded_io_async::{Read, Write};

use embassy_rp::peripherals::{UART0, UART1};
use embassy_rp::uart::{Async, BufferedUart, BufferedUartTx, Error as UartError, UartTx};
use embassy_rp::usb::Driver;
use midly::io::Cursor;
use midly::live::{LiveEvent, SystemCommon, SystemRealtime};
use midly::stream::MidiStream;
use midly::MidiMessage;

use crate::{tasks::clock::CLOCK_PUBSUB, CONFIG_CHANGE_WATCH};

use super::clock::ClockEvent;

midly::stack_buffer! {
    struct UartRxBuffer([u8; 3]);
}

const RUNNING_STATUS_DEBOUNCE: Duration = Duration::from_millis(200);
const MIDI_CHANNEL_SIZE: usize = 16;

pub type MidiSender =
    Sender<'static, CriticalSectionRawMutex, LiveEvent<'static>, MIDI_CHANNEL_SIZE>;
pub static MIDI_CHANNEL: Channel<CriticalSectionRawMutex, LiveEvent<'_>, MIDI_CHANNEL_SIZE> =
    Channel::new();

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

async fn write_msg_to_usb<'a>(
    usb_tx: &mut UsbSender<'a, Driver<'a, USB>>,
    midi_ev: LiveEvent<'a>,
) -> Result<(), TimeoutError> {
    let mut usb_buf = [0_u8; 4];
    usb_buf[0] = cin_from_live_event(&midi_ev) as u8;
    let mut usb_cursor = Cursor::new(&mut usb_buf[1..]);
    midi_ev.write(&mut usb_cursor).unwrap();
    let _ = with_timeout(
        // 1ms of timeout should be enough for USB host to have acknowledged
        Duration::from_millis(1),
        // Write including USB-MIDI CIN
        usb_tx.write_packet(&usb_buf),
    )
    .await?;
    Ok(())
}

async fn write_msg_to_uart(
    uart1_tx: &mut BufferedUartTx<'static, UART1>,
    midi_ev: LiveEvent<'_>,
) -> Result<(), UartError> {
    let mut ser_buf = [0_u8; 3];
    let mut ser_cursor = Cursor::new(&mut ser_buf);
    midi_ev.write(&mut ser_cursor).unwrap();
    let bytes_written = ser_cursor.cursor();
    uart1_tx.write_all(&ser_buf[..bytes_written]).await?;
    uart1_tx.flush().await?;
    Ok(())
}

pub async fn start_midi_loops<'a>(
    usb_midi: MidiClass<'a, Driver<'a, USB>>,
    uart0: UartTx<'static, UART0, Async>,
    uart1: BufferedUart<'static, UART1>,
) {
    let (mut usb_tx, mut usb_rx) = usb_midi.split();
    let uart0_tx: Mutex<NoopRawMutex, UartTx<'static, UART0, Async>> = Mutex::new(uart0);
    let (mut uart1_tx, mut uart1_rx) = uart1.split();
    let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();
    let mut config_receiver = CONFIG_CHANGE_WATCH.receiver().unwrap();
    let initial_config = config_receiver.try_get().unwrap();
    let config: Mutex<NoopRawMutex, GlobalConfig> = Mutex::new(initial_config);

    let midi_tx = async {
        // TODO: Do not try to send midi message to USB when not connected
        // usb_tx.wait_connection().await;
        // TODO: Deal with backpressure as well (do it on core b maybe?)
        // See https://claude.ai/chat/1a702bdf-b1f9-4d52-a004-aa221cbb4642 for improving this

        loop {
            let midi_ev = MIDI_CHANNEL.receive().await;

            let (_, _) = join(
                write_msg_to_uart(&mut uart1_tx, midi_ev),
                write_msg_to_usb(&mut usb_tx, midi_ev),
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
                match LiveEvent::parse(data) {
                    Ok(event) => {
                        let cfg = CONFIG_CHANGE_WATCH.try_get().unwrap();
                        match event {
                            LiveEvent::Realtime(msg) => match msg {
                                SystemRealtime::TimingClock => {
                                    if let ClockSrc::MidiUsb = cfg.clock_src {
                                        clock_publisher.publish(ClockEvent::Tick).await;
                                    }
                                }
                                SystemRealtime::Start => {
                                    if let ClockSrc::MidiUsb = cfg.reset_src {
                                        clock_publisher.publish(ClockEvent::Start).await;
                                    }
                                }
                                SystemRealtime::Stop => {
                                    if let ClockSrc::MidiUsb = cfg.reset_src {
                                        clock_publisher.publish(ClockEvent::Reset).await;
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
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
        let mut uart_rx_buffer = [0u8; 16];
        let mut midi_stream = MidiStream::<UartRxBuffer>::default();
        loop {
            if let Ok(bytes_read) = uart1_rx.read(&mut uart_rx_buffer).await {
                let cfg = CONFIG_CHANGE_WATCH.try_get().unwrap();
                midi_stream.feed(
                    &uart_rx_buffer[..bytes_read],
                    |event: LiveEvent| match event {
                        LiveEvent::Realtime(msg) => match msg {
                            SystemRealtime::TimingClock => {
                                if let ClockSrc::MidiIn = cfg.clock_src {
                                    clock_publisher.publish_immediate(ClockEvent::Tick);
                                }
                            }
                            SystemRealtime::Start => {
                                if let ClockSrc::MidiIn = cfg.reset_src {
                                    clock_publisher.publish_immediate(ClockEvent::Start);
                                }
                            }
                            SystemRealtime::Stop => {
                                if let ClockSrc::MidiIn = cfg.reset_src {
                                    clock_publisher.publish_immediate(ClockEvent::Reset);
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                );
            }
        }
    };

    let config_fut = async {
        loop {
            let new_config = config_receiver.changed().await;
            let mut cfg = config.lock().await;
            *cfg = new_config;
        }
    };

    join4(midi_tx, usb_rx, uart_rx, config_fut).await;
}

fn cin_from_live_event(midi_ev: &LiveEvent) -> CodeIndexNumber {
    match midi_ev {
        LiveEvent::Realtime(..) => CodeIndexNumber::SingleByte,
        LiveEvent::Midi { message, .. } => match message {
            MidiMessage::NoteOn { .. } => CodeIndexNumber::NoteOn,
            MidiMessage::NoteOff { .. } => CodeIndexNumber::NoteOff,
            MidiMessage::Aftertouch { .. } => CodeIndexNumber::KeyPressure,
            MidiMessage::ChannelAftertouch { .. } => CodeIndexNumber::ChannelPressure,
            MidiMessage::ProgramChange { .. } => CodeIndexNumber::ProgramChange,
            MidiMessage::Controller { .. } => CodeIndexNumber::ControlChange,
            MidiMessage::PitchBend { .. } => CodeIndexNumber::PitchBendChange,
        },
        LiveEvent::Common(common_message) => match common_message {
            SystemCommon::SysEx(data) => {
                // TODO: Implement stateful SysEx CIN determination once needed
                if data.is_empty() {
                    CodeIndexNumber::SysExEndsNext3
                } else {
                    CodeIndexNumber::SysExStarts
                }
            }
            SystemCommon::SongSelect(..) => CodeIndexNumber::SystemCommonLen2,
            SystemCommon::TuneRequest => CodeIndexNumber::SingleByte,
            SystemCommon::Undefined(..) => CodeIndexNumber::MiscFunction,
            SystemCommon::SongPosition(..) => CodeIndexNumber::SystemCommonLen3,
            SystemCommon::MidiTimeCodeQuarterFrame(..) => CodeIndexNumber::SystemCommonLen2,
        },
    }
}
