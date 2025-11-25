use defmt::info;
use embassy_futures::{
    join::join,
    select::{select, select3, Either, Either3},
};
use embassy_rp::{
    peripherals::USB,
    uart::{Async, BufferedUart, BufferedUartTx, Error as UartError, UartTx},
    usb::Driver,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::{Channel, Sender},
    pubsub::{PubSubChannel, Subscriber},
};
use embassy_time::{with_timeout, Duration, Ticker, TimeoutError};
use embassy_usb::class::midi::{MidiClass, Sender as UsbSender};
use embedded_io_async::{Read, Write};
use heapless::{Deque, Vec};
use midly::{
    io::Cursor,
    live::{LiveEvent, SystemCommon, SystemRealtime},
    stream::MidiStream,
    MidiMessage,
};

use libfp::{ClockSrc, GLOBAL_CHANNELS};

use crate::tasks::clock::{ClockInEvent, CLOCK_IN_CHANNEL};

midly::stack_buffer! {
    struct MidiStreamBuffer([u8; 64]);
}

const MIDI_CHANNEL_SIZE: usize = 16;
const MIDI_APP_QUEUE_SIZE: usize = 16;
const MIDI_PUBSUB_SIZE: usize = 64;
// Max apps
const MIDI_PUBSUB_SUBS: usize = GLOBAL_CHANNELS;
// Only one, from here
const MIDI_PUBSUB_SENDERS: usize = 1;

#[derive(Clone, Copy)]
pub enum MidiClockTarget {
    Usb,
    Uart,
    Both,
}

#[derive(Clone, Copy)]
pub enum MidiOutEvent {
    Event(LiveEvent<'static>),
    Clock(SystemRealtime, MidiClockTarget),
}

pub static MIDI_CHANNEL: Channel<CriticalSectionRawMutex, MidiOutEvent, MIDI_CHANNEL_SIZE> =
    Channel::new();

// Channel for apps (Core 1) to send MIDI to the distributor task (Core 1)
pub static APP_MIDI_CHANNEL: Channel<
    ThreadModeRawMutex,
    (usize, LiveEvent<'static>),
    MIDI_CHANNEL_SIZE,
> = Channel::new();

pub type AppMidiSender =
    Sender<'static, ThreadModeRawMutex, (usize, LiveEvent<'static>), MIDI_CHANNEL_SIZE>;

// Define the type once
pub type MidiPubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    LiveEvent<'static>,
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

// Instantiate specific channels for your sources
pub static MIDI_USB_PUBSUB: MidiPubSubChannel = PubSubChannel::new();
pub static MIDI_DIN_PUBSUB: MidiPubSubChannel = PubSubChannel::new();

#[embassy_executor::task]
pub async fn midi_distributor() {
    let mut app_queues: [Deque<LiveEvent<'static>, MIDI_APP_QUEUE_SIZE>; 16] =
        core::array::from_fn(|_| Deque::new());
    let mut last_app_id: usize = 0;
    let midi_out_sender = MIDI_CHANNEL.sender();
    let app_midi_receiver = APP_MIDI_CHANNEL.receiver();
    let mut ticker = Ticker::every(Duration::from_millis(2));

    loop {
        match select(app_midi_receiver.receive(), ticker.next()).await {
            // A new message from an app has arrived, enqueue it.
            Either::First((start_channel, ev)) => {
                if !app_queues[start_channel].is_full() {
                    app_queues[start_channel].push_back(ev).unwrap();
                }
            }
            // The 1ms throttle timer has fired, send one message.
            Either::Second(_) => {
                // Find the next app with a message in its queue (round-robin)
                for i in 0..16 {
                    let app_idx = (last_app_id + 1 + i) % 16;
                    if let Some(ev) = app_queues[app_idx].pop_front() {
                        midi_out_sender.send(MidiOutEvent::Event(ev)).await;
                        last_app_id = app_idx;
                        break; // Stop after sending one message
                    }
                }
            }
        }
    }
}

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
    uart0_tx: &mut UartTx<'static, Async>,
    uart1_tx: &mut BufferedUartTx,
    midi_ev: LiveEvent<'_>,
) -> Result<(), UartError> {
    let mut ser_buf = [0_u8; 3];
    let mut ser_cursor = Cursor::new(&mut ser_buf);
    midi_ev.write(&mut ser_cursor).unwrap();
    let bytes_written = ser_cursor.cursor();
    let written_slice = &ser_buf[..bytes_written];

    // Write to both UARTs concurrently
    let (res0, res1) = join(
        uart0_tx.write(written_slice),
        uart1_tx.write_all(written_slice),
    )
    .await;

    res0?;
    res1?;

    uart1_tx.flush().await?;
    Ok(())
}

pub async fn start_midi_loops<'a>(
    usb_midi: MidiClass<'a, Driver<'a, USB>>,
    mut uart0_tx: UartTx<'static, Async>,
    uart1: BufferedUart,
) {
    let (mut usb_tx, mut usb_rx) = usb_midi.split();
    let (mut uart1_tx, mut uart1_rx) = uart1.split();
    let clock_in_sender = CLOCK_IN_CHANNEL.sender();
    let din_publisher = MIDI_DIN_PUBSUB.publisher().unwrap();
    let usb_publisher = MIDI_USB_PUBSUB.publisher().unwrap();

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
            Either3::First(midi_out_event) => {
                let (event, target) = match midi_out_event {
                    MidiOutEvent::Event(ev) => (ev, MidiClockTarget::Both),
                    MidiOutEvent::Clock(msg, target) => (LiveEvent::Realtime(msg), target),
                };

                // TODO: Do not try to send midi message to USB when not connected
                // usb_tx.wait_connection().await;
                match target {
                    MidiClockTarget::Both => {
                        let (_, _) = join(
                            write_msg_to_uart(&mut uart0_tx, &mut uart1_tx, event),
                            write_msg_to_usb(&mut usb_tx, event),
                        )
                        .await;
                    }
                    MidiClockTarget::Uart => {
                        let _ = write_msg_to_uart(&mut uart0_tx, &mut uart1_tx, event).await;
                    }
                    MidiClockTarget::Usb => {
                        let _ = write_msg_to_usb(&mut usb_tx, event).await;
                    }
                }
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

                        match LiveEvent::parse(msg) {
                            Ok(event) => match event {
                                LiveEvent::Realtime(msg) => match msg {
                                    SystemRealtime::TimingClock => {
                                        clock_in_sender
                                            .send(ClockInEvent::Tick(ClockSrc::MidiUsb))
                                            .await;
                                    }
                                    SystemRealtime::Start => {
                                        clock_in_sender
                                            .send(ClockInEvent::Start(ClockSrc::MidiUsb))
                                            .await;
                                    }
                                    SystemRealtime::Continue => {
                                        clock_in_sender
                                            .send(ClockInEvent::Continue(ClockSrc::MidiUsb))
                                            .await;
                                    }
                                    SystemRealtime::Reset => {
                                        clock_in_sender
                                            .send(ClockInEvent::Reset(ClockSrc::MidiUsb))
                                            .await;
                                    }
                                    SystemRealtime::Stop => {
                                        clock_in_sender
                                            .send(ClockInEvent::Stop(ClockSrc::MidiUsb))
                                            .await;
                                    }
                                    _ => {}
                                },
                                _ => {
                                    usb_publisher.publish_immediate(event.to_static());
                                }
                            },
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

                    for event in uart_events.iter() {
                        match event {
                            LiveEvent::Realtime(msg) => match msg {
                                SystemRealtime::TimingClock => {
                                    clock_in_sender
                                        .send(ClockInEvent::Tick(ClockSrc::MidiIn))
                                        .await;
                                }
                                SystemRealtime::Start => {
                                    clock_in_sender
                                        .send(ClockInEvent::Start(ClockSrc::MidiIn))
                                        .await;
                                }
                                SystemRealtime::Stop => {
                                    clock_in_sender
                                        .send(ClockInEvent::Stop(ClockSrc::MidiIn))
                                        .await;
                                }
                                SystemRealtime::Continue => {
                                    clock_in_sender
                                        .send(ClockInEvent::Continue(ClockSrc::MidiIn))
                                        .await;
                                }
                                SystemRealtime::Reset => {
                                    clock_in_sender
                                        .send(ClockInEvent::Reset(ClockSrc::MidiIn))
                                        .await;
                                }
                                _ => {}
                            },
                            _ => {
                                din_publisher.publish_immediate(event.to_static());
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
