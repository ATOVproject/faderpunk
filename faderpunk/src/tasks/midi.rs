use defmt::info;
use embassy_futures::{
    join::join,
    select::{select3, Either3},
};
use embassy_rp::{
    peripherals::USB,
    uart::{Async, BufferedUart, BufferedUartTx, Error as UartError, UartTx},
    usb::Driver,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::{with_timeout, Duration, TimeoutError};
use embassy_usb::class::midi::{MidiClass, Sender as UsbSender};
use embedded_io_async::{Read, Write};
use heapless::Vec;
use midly::{
    io::Cursor,
    live::{LiveEvent, SystemCommon, SystemRealtime},
    stream::MidiStream,
    MidiMessage,
};

use libfp::ClockSrc;

use crate::{
    events::{InputEvent, EVENT_PUBSUB},
    tasks::{clock::CLOCK_PUBSUB, global_config::get_global_config},
};

use super::clock::ClockEvent;

midly::stack_buffer! {
    struct MidiStreamBuffer([u8; 64]);
}

// 16 apps plus clock
const MIDI_CHANNEL_SIZE: usize = 17;

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
    uart1_tx: &mut BufferedUartTx,
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
    uart0: UartTx<'static, Async>,
    uart1: BufferedUart,
) {
    let (mut usb_tx, mut usb_rx) = usb_midi.split();
    let uart0_tx: Mutex<NoopRawMutex, UartTx<'static, Async>> = Mutex::new(uart0);
    let (mut uart1_tx, mut uart1_rx) = uart1.split();
    let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let mut usb_rx_buf = [0; 64];
    let mut uart_rx_buffer = [0u8; 64];
    let mut midi_stream = MidiStream::<MidiStreamBuffer>::default();
    let mut uart_events = Vec::<LiveEvent<'static>, 64>::new();

    loop {
        let selected = select3(
            MIDI_CHANNEL.receive(),
            usb_rx.read_packet(&mut usb_rx_buf),
            uart1_rx.read(&mut uart_rx_buffer),
        )
        .await;

        match selected {
            // MIDI TX
            Either3::First(midi_ev) => {
                // TODO: Do not try to send midi message to USB when not connected
                // usb_tx.wait_connection().await;
                // TODO: Deal with backpressure as well (do it on core b maybe?)
                let (_, _) = join(
                    write_msg_to_uart(&mut uart1_tx, midi_ev),
                    write_msg_to_usb(&mut usb_tx, midi_ev),
                )
                .await;
            }
            // USB RX
            Either3::Second(result) => {
                if let Ok(len) = result {
                    if len == 0 {
                        continue;
                    }
                    let packets = usb_rx_buf[..len].chunks_exact(4);
                    for packet in packets {
                        let msg_len = len_from_cin(packet[0]);
                        if msg_len == 0 {
                            continue;
                        }

                        let msg = &packet[1..1 + msg_len];
                        // MIDI-THRU to uart0
                        {
                            let mut tx = uart0_tx.lock().await;
                            tx.write(msg).await.unwrap();
                        }

                        // With this structure, you can now easily route from USB to other outputs
                        // For example:
                        // write_msg_to_uart(&mut uart1_tx, LiveEvent::parse(msg).unwrap()).await;

                        match LiveEvent::parse(msg) {
                            Ok(event) => {
                                let config = get_global_config();
                                match event {
                                    LiveEvent::Realtime(msg) => {
                                        match msg {
                                            SystemRealtime::TimingClock => {
                                                if let ClockSrc::MidiUsb = config.clock.clock_src {
                                                    clock_publisher.publish(ClockEvent::Tick).await;
                                                }
                                            }
                                            SystemRealtime::Start => {
                                                if let ClockSrc::MidiUsb = config.clock.reset_src {
                                                    clock_publisher
                                                        .publish(ClockEvent::Start)
                                                        .await;
                                                }
                                            }
                                            SystemRealtime::Stop => {
                                                if let ClockSrc::MidiUsb = config.clock.reset_src {
                                                    clock_publisher
                                                        .publish(ClockEvent::Reset)
                                                        .await;
                                                }
                                            }
                                            _ => {}
                                        }

                                        // Pass through all realtime events to UART
                                        let _ = write_msg_to_uart(&mut uart1_tx, event).await;
                                    }
                                    _ => {
                                        event_publisher
                                            .publish(InputEvent::MidiMsg(event.to_static()))
                                            .await;
                                    }
                                }
                            }
                            Err(_err) => {
                                info!("Error parsing USB MIDI. Len: {}, Data: {}", len, msg);
                            }
                        }
                    }
                }
            }
            // UART RX
            Either3::Third(result) => {
                if let Ok(bytes_read) = result {
                    if bytes_read == 0 {
                        continue;
                    }

                    uart_events.clear();
                    midi_stream.feed(&uart_rx_buffer[..bytes_read], |event| {
                        let _ = uart_events.push(event.to_static());
                    });

                    let config = get_global_config();
                    for event in uart_events.iter() {
                        match event {
                            LiveEvent::Realtime(msg) => {
                                match msg {
                                    SystemRealtime::TimingClock => {
                                        if let ClockSrc::MidiIn = config.clock.clock_src {
                                            clock_publisher.publish(ClockEvent::Tick).await;
                                        }
                                    }
                                    SystemRealtime::Start => {
                                        if let ClockSrc::MidiIn = config.clock.reset_src {
                                            clock_publisher.publish(ClockEvent::Start).await;
                                        }
                                    }
                                    SystemRealtime::Stop => {
                                        if let ClockSrc::MidiIn = config.clock.reset_src {
                                            clock_publisher.publish(ClockEvent::Reset).await;
                                        }
                                    }
                                    _ => {}
                                }
                                // Pass through all realtime events to USB
                                let _ = write_msg_to_usb(&mut usb_tx, *event).await;
                            }
                            _ => {
                                event_publisher
                                    .publish(InputEvent::MidiMsg(event.to_static()))
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }
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

fn len_from_cin(cin: u8) -> usize {
    match cin & 0x0f {
        0x5 | 0xf => 1,
        0x2 | 0x6 | 0xc | 0xd => 2,
        0x3 | 0x4 | 0x7 | 0x8 | 0x9 | 0xa | 0xb | 0xe => 3,
        _ => 0,
    }
}
