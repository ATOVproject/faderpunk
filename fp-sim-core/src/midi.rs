use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use midly::{live::LiveEvent, num::u7, MidiMessage};

use fp_core::events::EVENT_PUBSUB;
use fp_core::tasks::clock::SYNC_ENGINE_CHANNEL;
use fp_core::tasks::configure::{ConfigSink, ProtocolError, CONFIG_RX_CHANNEL};
use fp_core::tasks::global_config::get_global_config;
use fp_core::tasks::midi::{
    process_midi_event, MidiEventSource, MidiMsg, MidiOutEvent, NrpnTracker, MIDI_CHANNEL,
    MIDI_USB_PUBSUB,
};
use fp_sim_protocol::CoreToHost;
use libfp::sysex::SYSEX_HEADER;
use libfp::utils::scale_bits_12_14;
use libfp::{ClockSrc, MidiIn, MidiOutConfig, MidiOutMode};

use crate::{ipc, FIRMWARE_VERSION};

const RX_CHUNK: usize = 1024;

static PERF_RX: Channel<CriticalSectionRawMutex, heapless::Vec<u8, RX_CHUNK>, 16> = Channel::new();
static CONFIG_RX: Channel<CriticalSectionRawMutex, heapless::Vec<u8, RX_CHUNK>, 4> = Channel::new();

pub fn push_performance(bytes: &[u8]) -> bool {
    push_chunk(bytes, &PERF_RX)
}

pub fn push_config(bytes: &[u8]) -> bool {
    push_chunk(bytes, &CONFIG_RX)
}

fn push_chunk<const CAPACITY: usize>(
    bytes: &[u8],
    channel: &Channel<CriticalSectionRawMutex, heapless::Vec<u8, RX_CHUNK>, CAPACITY>,
) -> bool {
    let Ok(chunk) = heapless::Vec::from_slice(bytes) else {
        return false;
    };
    channel.try_send(chunk).is_ok()
}

fn send_event(event: LiveEvent<'_>) {
    let mut bytes = [0_u8; 3];
    let mut cursor = midly::io::Cursor::new(&mut bytes);
    event.write(&mut cursor).unwrap();
    let len = cursor.cursor();
    ipc::send(CoreToHost::PerformanceMidi(bytes[..len].to_vec()));
}

#[embassy_executor::task]
pub async fn midi_out_bridge() {
    let midi_receiver = MIDI_CHANNEL.receiver();

    loop {
        match midi_receiver.receive().await {
            MidiOutEvent::Event(MidiMsg::Live {
                event,
                target,
                source,
            }) => {
                let enabled = match source {
                    MidiEventSource::Local => {
                        let disabled = matches!(
                            get_global_config().midi.outs[0],
                            MidiOutConfig {
                                mode: MidiOutMode::MidiThru { .. },
                                ..
                            } | MidiOutConfig {
                                mode: MidiOutMode::None,
                                ..
                            }
                        );
                        target.0[0] && !disabled
                    }
                    MidiEventSource::Passthrough => target.0[0],
                };
                if enabled {
                    send_event(event);
                }
            }
            MidiOutEvent::Event(MidiMsg::Nrpn {
                channel,
                param,
                value,
                target,
            }) => {
                if !target.0[0] {
                    continue;
                }
                let value_14 = scale_bits_12_14(value);
                for (controller, cc_value) in [
                    (99, (param >> 7) as u8),
                    (98, (param & 0x7f) as u8),
                    (6, (value_14 >> 7) as u8),
                    (38, (value_14 & 0x7f) as u8),
                ] {
                    send_event(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller {
                            controller: u7::new(controller),
                            value: u7::new(cc_value),
                        },
                    });
                }
            }
            MidiOutEvent::Clock(msg) => {
                if msg.target.0[0] {
                    send_event(LiveEvent::Realtime(msg.event));
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn midi_in_bridge() {
    let sync_engine_sender = SYNC_ENGINE_CHANNEL.sender();
    let midi_sender = MIDI_CHANNEL.sender();
    let usb_publisher = MIDI_USB_PUBSUB.publisher().unwrap();
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();
    let mut nrpn_trackers: [NrpnTracker; 16] = Default::default();

    loop {
        let chunk = PERF_RX.receive().await;
        match LiveEvent::parse(&chunk) {
            Ok(event) => {
                let thru_targets = get_global_config().midi.outs.map(|config| {
                    matches!(
                        config,
                        MidiOutConfig {
                            mode: MidiOutMode::MidiThru {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        } | MidiOutConfig {
                            mode: MidiOutMode::MidiMerge {
                                sources: MidiIn([true, _]),
                                ..
                            },
                            ..
                        }
                    )
                });
                process_midi_event(
                    &event,
                    &usb_publisher,
                    &mut nrpn_trackers,
                    thru_targets,
                    ClockSrc::MidiUsb,
                    &sync_engine_sender,
                    &midi_sender,
                    &event_publisher,
                )
                .await;
            }
            Err(err) => log::debug!("Unparseable MIDI input ({} bytes): {err}", chunk.len()),
        }
    }
}

#[embassy_executor::task]
pub async fn config_in_bridge() {
    let mut sysex = Vec::new();
    let mut in_sysex = false;

    loop {
        let chunk = CONFIG_RX.receive().await;
        let mut bytes: &[u8] = &chunk;

        while !bytes.is_empty() {
            if !in_sysex {
                let Some(start) = bytes.iter().position(|&byte| byte == 0xf0) else {
                    break;
                };
                sysex.clear();
                in_sysex = true;
                bytes = &bytes[start..];
            }
            match bytes.iter().position(|&byte| byte == 0xf7) {
                Some(end) => {
                    sysex.extend_from_slice(&bytes[..end]);
                    in_sysex = false;
                    handle_config_sysex(&sysex);
                    bytes = &bytes[end + 1..];
                }
                None => {
                    sysex.extend_from_slice(bytes);
                    break;
                }
            }
        }
    }
}

fn handle_config_sysex(frame: &[u8]) {
    let Some(body) = frame.strip_prefix(&[0xf0]) else {
        return;
    };
    if !body.starts_with(&SYSEX_HEADER) {
        return;
    }
    match heapless::Vec::from_slice(body) {
        Ok(frame) => {
            if CONFIG_RX_CHANNEL.try_send(frame).is_err() {
                log::warn!("Config RX channel full, dropping frame");
            }
        }
        Err(()) => log::warn!("Config frame too large, dropping"),
    }
}

struct IpcConfigSink;

impl ConfigSink for IpcConfigSink {
    async fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProtocolError> {
        ipc::send(CoreToHost::ConfigMidi(frame.to_vec()));
        Ok(())
    }
}

#[embassy_executor::task]
pub async fn config_loop() {
    fp_core::tasks::configure::start_config_loop(IpcConfigSink, FIRMWARE_VERSION).await
}
