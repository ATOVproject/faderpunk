use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Duration};
use heapless::Vec;
use postcard::{from_bytes, to_slice};

use libfp::sysex::{
    pack_7bit, unpack_7bit, MAX_PLAIN_SIZE, MAX_SYSEX_FRAME, SYSEX_EOX, SYSEX_HEADER, SYSEX_START,
};
use libfp::{
    AuxJackMode, ConfigMsgIn, ConfigMsgOut, Layout, Value, APP_MAX_PARAMS, GLOBAL_CHANNELS,
};
use max11300::config::{ConfigMode0, ConfigMode3, ConfigMode5, Mode, Port, DACRANGE};
use portable_atomic::Ordering;

use crate::apps::{get_channels, get_config, REGISTERED_APP_IDS};
use crate::layout::{EvictionCmd, LAYOUT_EVICTION_REQ, LAYOUT_EVICTION_RES, LAYOUT_WATCH};
use crate::storage::factory_reset;
use crate::tasks::clock::{VOCT_MEASURE_REQ, VOCT_MEASURE_RES};
use crate::tasks::global_config::{get_global_config, GLOBAL_CONFIG_WATCH};
use crate::tasks::max::{MaxCmd, MAX_CHANNEL, MAX_VALUES_DAC};
use crate::tasks::midi::{SharedUsbSender, CONFIG_CABLE};
use crate::version::FIRMWARE_VERSION;

use super::transport::USB_MAX_PACKET_SIZE;

/// Buffer size for one reassembled config SysEx frame body (header + packed
/// payload, without F0/F7). Slightly above MAX_SYSEX_FRAME for headroom.
pub const CONFIG_FRAME_BUF: usize = 640;

/// Complete config SysEx frame bodies from the USB MIDI RX path
/// (tasks/midi.rs). The protocol is strictly request/response, so depth 1
/// suffices.
pub static CONFIG_RX_CHANNEL: Channel<CriticalSectionRawMutex, Vec<u8, CONFIG_FRAME_BUF>, 1> =
    Channel::new();

/// Per-packet write timeout for config responses. Generous compared to the
/// 1ms performance-MIDI timeout: config frames must not be silently
/// truncated, but a stalled host must not block the USB sender forever.
const CONFIG_WRITE_TIMEOUT_MS: u64 = 500;
/// Multi-message response timeout for app param collection
const APP_PARAM_TIMEOUT_MS: u64 = 1000;

pub enum AppParamCmd {
    SetAppParams {
        values: [Option<Value>; APP_MAX_PARAMS],
    },
    RequestParamValues,
}

pub static APP_PARAM_SIGNALS: [Signal<CriticalSectionRawMutex, AppParamCmd>; GLOBAL_CHANNELS] =
    [const { Signal::new() }; GLOBAL_CHANNELS];

pub static APP_PARAM_CHANNEL: Channel<
    CriticalSectionRawMutex,
    (u8, Vec<Value, APP_MAX_PARAMS>),
    GLOBAL_CHANNELS,
> = Channel::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ProtocolError {
    BufferTooSmall,
    DecodingError,
    EncodingError,
    TransmissionError,
    CorruptedMessage,
    Timeout,
}

pub async fn start_config_loop<'a>(usb_tx: &'a SharedUsbSender<'a>) {
    let mut proto = ConfigTransport::new(usb_tx);
    let mut layout_receiver = LAYOUT_WATCH.receiver().unwrap();
    let mut layout = layout_receiver.get().await;
    let mut pending_voct_eviction: Option<(u8, EvictedApp)> = None;
    loop {
        let msg = match proto.read_msg().await {
            Ok(msg) => msg,
            Err(err) => {
                defmt::warn!("Dropping invalid config frame: {}", err);
                continue;
            }
        };
        let res = match msg {
            ConfigMsgIn::Ping => proto.send_msg(ConfigMsgOut::Pong).await,
            ConfigMsgIn::GetVersion => {
                let (major, minor, patch) = FIRMWARE_VERSION;
                proto
                    .send_msg(ConfigMsgOut::Version {
                        major,
                        minor,
                        patch,
                    })
                    .await
            }
            ConfigMsgIn::GetAllApps => {
                let configs = REGISTERED_APP_IDS.map(get_config);
                let mut res = proto
                    .send_msg(ConfigMsgOut::BatchMsgStart(configs.len()))
                    .await;
                for (app_id, channels, config_meta) in configs.into_iter().flatten() {
                    if res.is_err() {
                        break;
                    }
                    res = proto
                        .send_msg(ConfigMsgOut::AppConfig(app_id, channels, config_meta))
                        .await;
                }
                if res.is_ok() {
                    res = proto.send_msg(ConfigMsgOut::BatchMsgEnd).await;
                }
                res
            }
            ConfigMsgIn::GetLayout => proto.send_msg(ConfigMsgOut::Layout(layout.clone())).await,
            ConfigMsgIn::GetGlobalConfig => {
                let config = get_global_config();
                proto.send_msg(ConfigMsgOut::GlobalConfig(config)).await
            }
            ConfigMsgIn::GetAppParams { layout_id } => {
                APP_PARAM_SIGNALS[layout_id as usize].signal(AppParamCmd::RequestParamValues);
                if let Ok((res_layout_id, values)) = with_timeout(
                    Duration::from_millis(APP_PARAM_TIMEOUT_MS),
                    APP_PARAM_CHANNEL.receive(),
                )
                .await
                {
                    proto
                        .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                        .await
                } else {
                    Ok(())
                }
            }
            ConfigMsgIn::SetAppParams { layout_id, values } => {
                APP_PARAM_SIGNALS[layout_id as usize].signal(AppParamCmd::SetAppParams { values });
                if let Ok((res_layout_id, values)) = with_timeout(
                    Duration::from_millis(APP_PARAM_TIMEOUT_MS),
                    APP_PARAM_CHANNEL.receive(),
                )
                .await
                {
                    proto
                        .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                        .await
                } else {
                    Ok(())
                }
            }
            ConfigMsgIn::GetAllAppParams => {
                let layout_ids = layout.get_layout_ids();
                let app_count = layout_ids.len();

                let mut res = proto.send_msg(ConfigMsgOut::BatchMsgStart(app_count)).await;

                if app_count > 0 && res.is_ok() {
                    for id in layout_ids {
                        APP_PARAM_SIGNALS[id as usize].signal(AppParamCmd::RequestParamValues);
                    }
                    let receiver = async {
                        for _ in 0..app_count {
                            let (res_layout_id, values) = APP_PARAM_CHANNEL.receive().await;
                            proto
                                .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                                .await?;
                        }
                        Ok(())
                    };

                    if let Ok(receiver_res) =
                        with_timeout(Duration::from_millis(APP_PARAM_TIMEOUT_MS), receiver).await
                    {
                        res = receiver_res;
                    }
                }

                if res.is_ok() {
                    res = proto.send_msg(ConfigMsgOut::BatchMsgEnd).await;
                }
                res
            }
            ConfigMsgIn::SetGlobalConfig(mut global_config) => {
                global_config.validate();
                let sender = GLOBAL_CONFIG_WATCH.sender();
                sender.send(global_config);
                Ok(())
            }
            ConfigMsgIn::SetLayout(mut new_layout) => {
                new_layout.validate(get_channels);
                let sender = LAYOUT_WATCH.sender();
                let res = proto
                    .send_msg(ConfigMsgOut::Layout(new_layout.clone()))
                    .await;
                layout = new_layout.clone();
                sender.send(new_layout);
                res
            }
            ConfigMsgIn::FactoryReset => {
                factory_reset().await;
                Ok(())
            }
            ConfigMsgIn::MeasureVoOct {
                output_jack,
                aux_input,
                dac_counts,
            } => {
                handle_measure_voct(
                    &mut proto,
                    &layout,
                    &mut pending_voct_eviction,
                    output_jack,
                    aux_input,
                    dac_counts,
                )
                .await
            }
            ConfigMsgIn::SetVoOctOutput {
                output_jack,
                dac_counts,
            } => {
                handle_set_voct_output(
                    &mut proto,
                    &layout,
                    &mut pending_voct_eviction,
                    output_jack,
                    dac_counts,
                )
                .await
            }
            ConfigMsgIn::ReleaseVoOctOutput { output_jack } => {
                handle_release_voct_output(&mut proto, &mut pending_voct_eviction, output_jack)
                    .await
            }
        };
        if let Err(err) = res {
            defmt::warn!("Failed to send config response: {}", err);
        }
    }
}

/// Config protocol transport: reads reassembled SysEx frame bodies from
/// CONFIG_RX_CHANNEL and writes responses as cable-1 SysEx over the shared
/// USB-MIDI sender. Wire format: see libfp::sysex.
struct ConfigTransport<'a> {
    usb_tx: &'a SharedUsbSender<'a>,
    plain_buf: [u8; MAX_PLAIN_SIZE],
    frame_buf: [u8; MAX_SYSEX_FRAME],
}

/// (app_id, start_channel, channels, layout_id) of an app temporarily evicted
/// from its jack for V/Oct calibration.
type EvictedApp = (u8, usize, usize, u8);

/// Find the app (if any) currently occupying `jack`.
fn find_jack_owner(layout: &Layout, jack: u8) -> Option<EvictedApp> {
    let jack = jack as usize;
    layout.iter().find(|&(_, start_channel, channels, _)| {
        jack >= start_channel && jack < start_channel + channels
    })
}

/// If `jack` is occupied by a running app, temporarily evict it (a scoped,
/// non-persisting despawn that doesn't touch the persisted layout) and return
/// its identity so it can be restored with `restore_jack_owner` once
/// calibration is done. Returns `None` (no-op) if the jack is unassigned.
async fn evict_jack_owner(layout: &Layout, jack: u8) -> Option<EvictedApp> {
    let owner = find_jack_owner(layout, jack)?;
    let (_, start_channel, _, _) = owner;
    LAYOUT_EVICTION_REQ.signal(EvictionCmd::Evict(start_channel));
    LAYOUT_EVICTION_RES.wait().await;
    Some(owner)
}

/// Respawn a previously-evicted app back onto its original channel(s).
async fn restore_jack_owner(evicted: EvictedApp) {
    let (app_id, start_channel, channels, layout_id) = evicted;
    LAYOUT_EVICTION_REQ.signal(EvictionCmd::Restore(
        start_channel,
        app_id,
        channels,
        layout_id,
    ));
    LAYOUT_EVICTION_RES.wait().await;
}

/// Set the output jack to 0-10V DAC mode, write `dac_counts`, signal the
/// clock task to measure frequency on the chosen AUX pin, wait for the result,
/// then release the jack (Mode1 high-Z). If an app is running on `output_jack`,
/// it is temporarily evicted and respawned afterward so calibration never
/// leaves it stranded in high-Z. Shares `pending_eviction` with
/// `handle_set_voct_output`/`handle_release_voct_output` (rather than
/// tracking its own eviction locally) so the two flows can never disagree
/// about whether a jack is currently evicted.
async fn handle_measure_voct(
    proto: &mut ConfigTransport<'_>,
    layout: &Layout,
    pending_eviction: &mut Option<(u8, EvictedApp)>,
    output_jack: u8,
    aux_input: u8,
    dac_counts: u16,
) -> Result<(), ProtocolError> {
    let aux_idx = aux_input as usize;
    if aux_idx > 2 || output_jack as usize >= 16 {
        return proto.send_msg(ConfigMsgOut::VoOctCalError).await;
    }

    let port = match Port::try_from(output_jack as usize) {
        Ok(p) => p,
        Err(_) => {
            return proto.send_msg(ConfigMsgOut::VoOctCalError).await;
        }
    };

    // This handler always fully resolves its own eviction before returning,
    // so start from a clean slate: settle any eviction left pending by the
    // manual flow first (this handler doesn't reuse it, since it always
    // restores at the end regardless).
    if let Some((_, evicted)) = pending_eviction.take() {
        restore_jack_owner(evicted).await;
    }
    let evicted = evict_jack_owner(layout, output_jack).await;

    // If the AUX jack is configured as ClockOut or ResetOut its MAX11300 port
    // (17 + aux_idx) is in Mode3 (GPO) and will drive the jack voltage, which
    // would corrupt frequency measurements. Force it to Mode0 (high-Z) for the
    // duration of the measurement, then restore it afterwards.
    let aux_port = Port::try_from(17 + aux_idx).unwrap();
    let aux_was_active = matches!(
        get_global_config().aux[aux_idx],
        AuxJackMode::ClockOut(_) | AuxJackMode::ResetOut
    );
    if aux_was_active {
        MAX_CHANNEL
            .send(MaxCmd::ConfigurePort {
                port: aux_port,
                mode: Mode::Mode0(ConfigMode0),
                gpo_level: None,
            })
            .await;
    }

    // Configure the output jack to 0-10V DAC mode.
    MAX_CHANNEL
        .send(MaxCmd::ConfigurePort {
            port,
            mode: Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v)),
            gpo_level: None,
        })
        .await;

    // Keep re-writing dac_counts every 5 ms as a defensive guard (in case
    // eviction above was a no-op, e.g. a future caller skips it). After
    // 300 ms the VCO has settled; we then trigger the frequency measurement
    // and wait for the result. The drive loop is cancelled automatically
    // when select() returns (never() branch wins as soon as
    // drive_and_measure completes).
    let drive_and_measure = async {
        MAX_VALUES_DAC[output_jack as usize].store(dac_counts, Ordering::Relaxed);
        embassy_time::Timer::after_millis(300).await;
        VOCT_MEASURE_REQ[aux_idx].signal(());
        with_timeout(Duration::from_secs(30), VOCT_MEASURE_RES.wait()).await
    };
    let keep_driving = async {
        loop {
            MAX_VALUES_DAC[output_jack as usize].store(dac_counts, Ordering::Relaxed);
            embassy_time::Timer::after_millis(5).await;
        }
    };

    let freq_res = match select(keep_driving, drive_and_measure).await {
        Either::Second(r) => r,
        Either::First(_) => unreachable!(),
    };

    let res = match freq_res {
        Ok(Ok(freq_hz)) => proto.send_msg(ConfigMsgOut::VoOctFrequency { freq_hz }).await,
        _ => proto.send_msg(ConfigMsgOut::VoOctCalError).await,
    };

    // Release the output jack to high-Z (Mode 0) so apps can reclaim it.
    MAX_CHANNEL
        .send(MaxCmd::ConfigurePort {
            port,
            mode: Mode::Mode0(ConfigMode0),
            gpo_level: None,
        })
        .await;

    // Restore the AUX port to GPO mode if it was active before measurement.
    if aux_was_active {
        MAX_CHANNEL
            .send(MaxCmd::ConfigurePort {
                port: aux_port,
                mode: Mode::Mode3(ConfigMode3),
                gpo_level: Some(2048),
            })
            .await;
    }

    if let Some(evicted) = evicted {
        restore_jack_owner(evicted).await;
    }
    // Always fully resolved by now, regardless of what pending_eviction held
    // on entry.
    *pending_eviction = None;

    res
}

/// Set `output_jack` to 0-10V DAC mode and write `dac_counts`, then hold it
/// there for the caller to measure manually (e.g. with an external frequency
/// counter). If an app is running on `output_jack`, it is temporarily
/// evicted; `pending_eviction` remembers it (jack, evicted app) across the
/// paired `ReleaseVoOctOutput` call, since the caller measures externally
/// with an arbitrary delay in between. Only one eviction is tracked at a
/// time, matching the configurator's manual flow (which only ever targets
/// one jack per session) — if a different jack is already pending, it is
/// restored first.
async fn handle_set_voct_output(
    proto: &mut ConfigTransport<'_>,
    layout: &Layout,
    pending_eviction: &mut Option<(u8, EvictedApp)>,
    output_jack: u8,
    dac_counts: u16,
) -> Result<(), ProtocolError> {
    let port = match Port::try_from(output_jack as usize) {
        Ok(p) if (output_jack as usize) < 16 => p,
        _ => {
            return proto.send_msg(ConfigMsgOut::VoOctOutputSet).await;
        }
    };

    if let Some((jack, evicted)) = pending_eviction.take() {
        if jack == output_jack {
            *pending_eviction = Some((jack, evicted));
        } else {
            restore_jack_owner(evicted).await;
        }
    }
    if pending_eviction.is_none() {
        if let Some(evicted) = evict_jack_owner(layout, output_jack).await {
            *pending_eviction = Some((output_jack, evicted));
        }
    }

    MAX_CHANNEL
        .send(MaxCmd::ConfigurePort {
            port,
            mode: Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v)),
            gpo_level: None,
        })
        .await;
    MAX_VALUES_DAC[output_jack as usize].store(dac_counts, Ordering::Relaxed);

    proto.send_msg(ConfigMsgOut::VoOctOutputSet).await
}

/// Release `output_jack` back to high-Z (Mode 0) so apps can reclaim it,
/// restoring whatever app `handle_set_voct_output` evicted for this jack.
async fn handle_release_voct_output(
    proto: &mut ConfigTransport<'_>,
    pending_eviction: &mut Option<(u8, EvictedApp)>,
    output_jack: u8,
) -> Result<(), ProtocolError> {
    if let Ok(port) = Port::try_from(output_jack as usize) {
        if (output_jack as usize) < 16 {
            MAX_CHANNEL
                .send(MaxCmd::ConfigurePort {
                    port,
                    mode: Mode::Mode0(ConfigMode0),
                    gpo_level: None,
                })
                .await;
        }
    }

    if let Some((jack, evicted)) = pending_eviction.take() {
        if jack == output_jack {
            restore_jack_owner(evicted).await;
        } else {
            *pending_eviction = Some((jack, evicted));
        }
    }

    proto.send_msg(ConfigMsgOut::VoOctOutputSet).await
}

impl<'a> ConfigTransport<'a> {
    fn new(usb_tx: &'a SharedUsbSender<'a>) -> Self {
        ConfigTransport {
            usb_tx,
            plain_buf: [0; MAX_PLAIN_SIZE],
            frame_buf: [0; MAX_SYSEX_FRAME],
        }
    }

    async fn read_msg(&mut self) -> Result<ConfigMsgIn, ProtocolError> {
        let frame = CONFIG_RX_CHANNEL.receive().await;
        let packed = frame
            .strip_prefix(&SYSEX_HEADER[..])
            .ok_or(ProtocolError::CorruptedMessage)?;
        let plain_len =
            unpack_7bit(packed, &mut self.plain_buf).map_err(|_| ProtocolError::DecodingError)?;
        if plain_len < 2 {
            return Err(ProtocolError::CorruptedMessage);
        }
        let payload_len = ((self.plain_buf[0] as usize) << 8) | self.plain_buf[1] as usize;
        if payload_len != plain_len - 2 {
            return Err(ProtocolError::CorruptedMessage);
        }
        from_bytes(&self.plain_buf[2..plain_len]).map_err(|_| ProtocolError::DecodingError)
    }

    async fn send_msg(&mut self, msg: ConfigMsgOut<'_>) -> Result<(), ProtocolError> {
        let payload_len = to_slice(&msg, &mut self.plain_buf[2..])
            .map_err(|_| ProtocolError::EncodingError)?
            .len();
        self.plain_buf[0] = ((payload_len >> 8) & 0xFF) as u8;
        self.plain_buf[1] = (payload_len & 0xFF) as u8;
        let plain_len = payload_len + 2;

        self.frame_buf[0] = SYSEX_START;
        self.frame_buf[1..1 + SYSEX_HEADER.len()].copy_from_slice(&SYSEX_HEADER);
        let packed_len = pack_7bit(
            &self.plain_buf[..plain_len],
            &mut self.frame_buf[1 + SYSEX_HEADER.len()..MAX_SYSEX_FRAME - 1],
        )
        .map_err(|_| ProtocolError::BufferTooSmall)?;
        let frame_len = 1 + SYSEX_HEADER.len() + packed_len + 1;
        self.frame_buf[frame_len - 1] = SYSEX_EOX;

        // Packetize into cable-1 USB-MIDI event packets, flushed per 64-byte
        // USB packet. The sender mutex is released between USB packets so
        // performance MIDI (cable 0) interleaves during long transfers.
        let mut usb_packet = [0u8; USB_MAX_PACKET_SIZE as usize];
        let mut usb_len = 0;
        let total_chunks = frame_len.div_ceil(3);
        let mut last_write_len = 0;
        for (i, chunk) in self.frame_buf[..frame_len].chunks(3).enumerate() {
            let last = i + 1 == total_chunks;
            let cin: u8 = if last {
                // SysEx ends with following 1/2/3 bytes
                match chunk.len() {
                    1 => 0x5,
                    2 => 0x6,
                    _ => 0x7,
                }
            } else {
                // SysEx starts or continues
                0x4
            };
            usb_packet[usb_len] = (CONFIG_CABLE << 4) | cin;
            usb_packet[usb_len + 1..usb_len + 4].fill(0);
            usb_packet[usb_len + 1..usb_len + 1 + chunk.len()].copy_from_slice(chunk);
            usb_len += 4;
            if usb_len == usb_packet.len() || last {
                write_usb_packet(self.usb_tx, &usb_packet[..usb_len]).await?;
                last_write_len = usb_len;
                usb_len = 0;
            }
        }
        if last_write_len == usb_packet.len() {
            // Terminate the bulk transfer with a ZLP after a full-size packet
            write_usb_packet(self.usb_tx, &[]).await?;
        }

        Ok(())
    }
}

async fn write_usb_packet(usb_tx: &SharedUsbSender<'_>, data: &[u8]) -> Result<(), ProtocolError> {
    let mut tx = usb_tx.lock().await;
    with_timeout(
        Duration::from_millis(CONFIG_WRITE_TIMEOUT_MS),
        tx.write_packet(data),
    )
    .await
    .map_err(|_| ProtocolError::Timeout)?
    .map_err(|_| ProtocolError::TransmissionError)
}
