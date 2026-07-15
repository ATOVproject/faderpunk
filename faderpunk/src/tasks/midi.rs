//! Physical MIDI transports: USB-MIDI (2 virtual cables) and DIN UARTs.
//! The portable MIDI plumbing (types, channels, NRPN/RX processing) lives in
//! `fp_core::tasks::midi`.

use defmt::info;
use embassy_futures::{
    join::join3,
    select::{select, select3, Either, Either3},
};
use embassy_rp::{
    peripherals::USB,
    uart::{Async, BufferedUartRx, BufferedUartTx, Error as UartError, UartTx},
    usb::Driver,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{with_timeout, Duration, TimeoutError};
use embassy_usb::class::midi::{Receiver as UsbReceiver, Sender as UsbSender};
use embedded_io_async::{Read, Write};
use heapless::Vec;
use midly::{io::Cursor, live::LiveEvent, stream::MidiStream};

use libfp::{ClockSrc, MidiIn, MidiOut, MidiOutConfig, MidiOutMode};

use fp_core::events::EVENT_PUBSUB;
use fp_core::tasks::clock::SYNC_ENGINE_CHANNEL;
use fp_core::tasks::configure::CONFIG_RX_CHANNEL;
use fp_core::tasks::global_config::GLOBAL_CONFIG_WATCH;
use fp_core::tasks::midi::{
    cin_from_live_event, len_from_cin, process_midi_event, MidiEventSource, MidiMsg, MidiOutEvent,
    NrpnTracker, SysExAssembler, MIDI_CHANNEL, MIDI_DIN_PUBSUB, MIDI_USB_PUBSUB,
};

/// Virtual USB-MIDI cable carrying the configurator SysEx protocol.
/// Cable 0 is performance MIDI.
pub const CONFIG_CABLE: u8 = 1;

/// Shared USB-MIDI sender: performance MIDI out and the config loop write
/// through the same endpoint, interleaving per 64-byte USB packet.
pub type SharedUsbSender<'a> = Mutex<NoopRawMutex, UsbSender<'a, Driver<'a, USB>>>;

midly::stack_buffer! {
    struct MidiStreamBuffer([u8; 64]);
}

/// Per-packet write timeout for performance MIDI. Must cover several USB
/// full-speed frames: embedded USB MIDI hosts may poll bulk IN endpoints on a
/// multi-millisecond tick, and a desktop host never comes close. Packets are
/// dropped on expiry so a stalled host cannot block DIN output.
const USB_WRITE_TIMEOUT_MS: u64 = 5;

async fn write_msg_to_usb<'a>(
    usb_tx: &SharedUsbSender<'a>,
    midi_ev: LiveEvent<'a>,
) -> Result<(), TimeoutError> {
    let mut usb_buf = [0_u8; 4];
    // Cable nibble 0 (performance MIDI) | CIN
    usb_buf[0] = cin_from_live_event(&midi_ev) as u8;
    let mut usb_cursor = Cursor::new(&mut usb_buf[1..]);
    midi_ev.write(&mut usb_cursor).unwrap();
    let _ = with_timeout(Duration::from_millis(USB_WRITE_TIMEOUT_MS), async {
        // Write including USB-MIDI CIN
        usb_tx.lock().await.write_packet(&usb_buf).await
    })
    .await?;
    Ok(())
}

async fn write_msg_to_uart0(
    uart0_tx: &mut UartTx<'static, Async>,
    midi_ev: LiveEvent<'_>,
) -> Result<(), UartError> {
    let mut ser_buf = [0_u8; 3];
    let mut ser_cursor = Cursor::new(&mut ser_buf);
    midi_ev.write(&mut ser_cursor).unwrap();
    let bytes_written = ser_cursor.cursor();
    uart0_tx.write(&ser_buf[..bytes_written]).await?;
    Ok(())
}

async fn write_msg_to_uart1(
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

pub async fn midi_out_task<'a>(
    usb_tx: &SharedUsbSender<'a>,
    mut uart0_tx: UartTx<'static, Async>,
    mut uart1_tx: BufferedUartTx,
) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let midi_receiver = MIDI_CHANNEL.receiver();

    let config = config_receiver.get().await;
    let mut disabled_outs_for_local = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru { .. },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::None,
                ..
            }
        )
    });

    loop {
        match select(midi_receiver.receive(), config_receiver.changed()).await {
            Either::First(midi_out_msg) => {
                match midi_out_msg {
                    MidiOutEvent::Event(MidiMsg::Live {
                        event,
                        mut target,
                        source,
                    }) => {
                        // Disable targets where we have a strict THRU port or no output.
                        // Only for local events; passthrough and clock are handled elsewhere.
                        if let MidiEventSource::Local = source {
                            for (i, disabled) in disabled_outs_for_local.iter().enumerate() {
                                target.0[i] = target.0[i] && !disabled;
                            }
                        }

                        let usb_fut = async {
                            if let MidiOut([true, _, _]) = target {
                                let _ = write_msg_to_usb(usb_tx, event).await;
                            }
                        };
                        let out1_fut = async {
                            if let MidiOut([_, true, _]) = target {
                                let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
                            }
                        };
                        let out2_fut = async {
                            if let MidiOut([_, _, true]) = target {
                                let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
                            }
                        };
                        join3(usb_fut, out1_fut, out2_fut).await;
                    }
                    MidiOutEvent::Event(MidiMsg::Nrpn {
                        channel,
                        param,
                        value,
                        mut target,
                    }) => {
                        use midly::{num::u7, MidiMessage};

                        use libfp::utils::scale_bits_12_14;
                        for (i, disabled) in disabled_outs_for_local.iter().enumerate() {
                            target.0[i] = target.0[i] && !disabled;
                        }
                        let value_14 = scale_bits_12_14(value);
                        let ccs: [LiveEvent<'static>; 4] = [
                            LiveEvent::Midi {
                                channel,
                                message: MidiMessage::Controller {
                                    controller: u7::new(99),
                                    value: u7::new((param >> 7) as u8),
                                },
                            },
                            LiveEvent::Midi {
                                channel,
                                message: MidiMessage::Controller {
                                    controller: u7::new(98),
                                    value: u7::new((param & 0x7F) as u8),
                                },
                            },
                            LiveEvent::Midi {
                                channel,
                                message: MidiMessage::Controller {
                                    controller: u7::new(6),
                                    value: u7::new((value_14 >> 7) as u8),
                                },
                            },
                            LiveEvent::Midi {
                                channel,
                                message: MidiMessage::Controller {
                                    controller: u7::new(38),
                                    value: u7::new((value_14 & 0x7F) as u8),
                                },
                            },
                        ];
                        for event in ccs {
                            if let MidiOut([true, _, _]) = target {
                                let _ = write_msg_to_usb(usb_tx, event).await;
                            }
                            if let MidiOut([_, true, _]) = target {
                                let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
                            }
                            if let MidiOut([_, _, true]) = target {
                                let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
                            }
                        }
                    }
                    MidiOutEvent::Clock(msg) => {
                        let event = LiveEvent::Realtime(msg.event);
                        let usb_fut = async {
                            if let MidiOut([true, _, _]) = msg.target {
                                let _ = write_msg_to_usb(usb_tx, event).await;
                            }
                        };
                        let out1_fut = async {
                            if let MidiOut([_, true, _]) = msg.target {
                                let _ = write_msg_to_uart1(&mut uart1_tx, event).await;
                            }
                        };
                        let out2_fut = async {
                            if let MidiOut([_, _, true]) = msg.target {
                                let _ = write_msg_to_uart0(&mut uart0_tx, event).await;
                            }
                        };
                        join3(usb_fut, out1_fut, out2_fut).await;
                    }
                }
            }
            Either::Second(new_config) => {
                disabled_outs_for_local = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru { .. },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::None,
                            ..
                        }
                    )
                });
            }
        }
    }
}

pub async fn midi_in_task<'a>(
    mut usb_rx: UsbReceiver<'a, Driver<'a, USB>>,
    mut uart1_rx: BufferedUartRx,
) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();

    let sync_engine_sender = SYNC_ENGINE_CHANNEL.sender();
    let midi_sender = MIDI_CHANNEL.sender();
    let din_publisher = MIDI_DIN_PUBSUB.publisher().unwrap();
    let usb_publisher = MIDI_USB_PUBSUB.publisher().unwrap();
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let mut usb_rx_buf = [0; 64];
    let mut uart_rx_buffer = [0u8; 64];
    let mut midi_stream = MidiStream::<MidiStreamBuffer>::default();
    let mut uart_events = Vec::<LiveEvent<'static>, 64>::new();
    let mut config_assembler = SysExAssembler::new();
    let mut usb_nrpn_trackers: [NrpnTracker; 16] = Default::default();
    let mut din_nrpn_trackers: [NrpnTracker; 16] = Default::default();

    let config = config_receiver.get().await;

    // Get outputs that forward from MIDI DIN
    let mut midi_passthru_from_din = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::MidiMerge {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            }
        )
    });

    // Get outputs that forward from MIDI USB
    let mut midi_passthru_from_usb = config.midi.outs.map(|c| {
        matches!(
            c,
            MidiOutConfig {
                mode: MidiOutMode::MidiThru {
                    sources: MidiIn([true, _]),
                    ..
                },
                ..
            } | MidiOutConfig {
                mode: MidiOutMode::MidiMerge {
                    sources: MidiIn([_, true]),
                    ..
                },
                ..
            }
        )
    });

    loop {
        match select3(
            usb_rx.read_packet(&mut usb_rx_buf),
            uart1_rx.read(&mut uart_rx_buffer),
            config_receiver.changed(),
        )
        .await
        {
            // USB RX
            Either3::First(result) => {
                if let Ok(len) = result {
                    if len == 0 {
                        continue;
                    }
                    let packets = usb_rx_buf[..len].chunks_exact(4);
                    for packet in packets {
                        let cable = packet[0] >> 4;
                        let msg_len = len_from_cin(packet[0]);
                        if cable == CONFIG_CABLE {
                            // Config cable: assemble SysEx frames for the
                            // config loop; anything else is ignored by design.
                            let cin = packet[0] & 0x0F;
                            if (0x4..=0x7).contains(&cin)
                                && config_assembler.feed(cin, &packet[1..1 + msg_len])
                            {
                                match Vec::from_slice(config_assembler.frame()) {
                                    Ok(frame) => {
                                        if CONFIG_RX_CHANNEL.try_send(frame).is_err() {
                                            defmt::warn!("Config RX channel full, dropping frame");
                                        }
                                    }
                                    Err(()) => {
                                        defmt::warn!("Config frame too large, dropping");
                                    }
                                }
                                config_assembler.clear();
                            }
                            continue;
                        }
                        if msg_len == 0 {
                            continue;
                        }

                        let msg = &packet[1..1 + msg_len];

                        match LiveEvent::parse(msg) {
                            Ok(event) => {
                                process_midi_event(
                                    &event,
                                    &usb_publisher,
                                    &mut usb_nrpn_trackers,
                                    midi_passthru_from_usb,
                                    ClockSrc::MidiUsb,
                                    &sync_engine_sender,
                                    &midi_sender,
                                    &event_publisher,
                                )
                                .await;
                            }
                            Err(_err) => {
                                info!("Error parsing USB MIDI. Len: {}, Data: {}", len, msg);
                            }
                        }
                    }
                }
            }
            // UART RX
            Either3::Second(result) => {
                if let Ok(bytes_read) = result {
                    if bytes_read == 0 {
                        continue;
                    }

                    uart_events.clear();
                    midi_stream.feed(&uart_rx_buffer[..bytes_read], |event| {
                        let _ = uart_events.push(event.to_static());
                    });

                    for event in uart_events.iter() {
                        process_midi_event(
                            event,
                            &din_publisher,
                            &mut din_nrpn_trackers,
                            midi_passthru_from_din,
                            ClockSrc::MidiIn,
                            &sync_engine_sender,
                            &midi_sender,
                            &event_publisher,
                        )
                        .await;
                    }
                }
            }
            Either3::Third(new_config) => {
                // Get outputs that forward from MIDI DIN
                midi_passthru_from_din = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        }
                    )
                });

                // Get outputs that forward from MIDI USB
                midi_passthru_from_usb = new_config.midi.outs.map(|c| {
                    matches!(
                        c,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([_, true]),
                                ..
                            },
                            ..
                        }
                    )
                });
            }
        }
    }
}
