//! Configurator protocol (SysEx over MIDI): message dispatch and framing.
//! Complete inbound frame bodies arrive via [`CONFIG_RX_CHANNEL`] (fed by the
//! firmware's USB-MIDI RX path or the simulator's virtual MIDI input);
//! outbound frames are written through the host-provided [`ConfigSink`]
//! (USB-MIDI cable-1 packets on hardware, a virtual MIDI port on the
//! simulator).

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Duration};
use heapless::Vec;
use postcard::{from_bytes, to_slice};

use libfp::sysex::{
    pack_7bit, unpack_7bit, MAX_PLAIN_SIZE, MAX_SYSEX_FRAME, SYSEX_EOX, SYSEX_HEADER, SYSEX_START,
};
use libfp::{ConfigMsgIn, ConfigMsgOut, Value, APP_MAX_PARAMS, GLOBAL_CHANNELS};

use crate::apps::{get_channels, get_config, REGISTERED_APP_IDS};
use crate::layout::LAYOUT_WATCH;
use crate::storage::factory_reset;
use crate::tasks::global_config::{get_global_config, GLOBAL_CONFIG_WATCH};

/// Buffer size for one reassembled config SysEx frame body (header + packed
/// payload, without F0/F7). Slightly above MAX_SYSEX_FRAME for headroom.
pub const CONFIG_FRAME_BUF: usize = 640;

/// Complete config SysEx frame bodies from the MIDI RX path. The protocol is
/// strictly request/response, so depth 1 suffices.
pub static CONFIG_RX_CHANNEL: Channel<CriticalSectionRawMutex, Vec<u8, CONFIG_FRAME_BUF>, 1> =
    Channel::new();

/// Multi-message response timeout for app param collection
const APP_PARAM_TIMEOUT_MS: u64 = 1000;

#[allow(clippy::large_enum_variant)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProtocolError {
    BufferTooSmall,
    DecodingError,
    EncodingError,
    TransmissionError,
    CorruptedMessage,
    Timeout,
}

/// Writes one complete outbound config SysEx frame (including the F0/F7
/// delimiters) to the transport.
#[allow(async_fn_in_trait)]
pub trait ConfigSink {
    async fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProtocolError>;
}

pub async fn start_config_loop<S: ConfigSink>(sink: S, firmware_version: (u8, u8, u8)) {
    let mut proto = ConfigTransport::new(sink);
    let mut layout_receiver = LAYOUT_WATCH.receiver().unwrap();
    let mut layout = layout_receiver.get().await;
    loop {
        let msg = match proto.read_msg().await {
            Ok(msg) => msg,
            Err(err) => {
                warn!("Dropping invalid config frame: {:?}", err);
                continue;
            }
        };
        let res = match msg {
            ConfigMsgIn::Ping => proto.send_msg(ConfigMsgOut::Pong).await,
            ConfigMsgIn::GetVersion => {
                let (major, minor, patch) = firmware_version;
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
            }
        };
        if let Err(err) = res {
            warn!("Failed to send config response: {:?}", err);
        }
    }
}

/// Config protocol transport: reads reassembled SysEx frame bodies from
/// CONFIG_RX_CHANNEL and writes responses through the [`ConfigSink`].
/// Wire format: see libfp::sysex.
struct ConfigTransport<S: ConfigSink> {
    sink: S,
    plain_buf: [u8; MAX_PLAIN_SIZE],
    frame_buf: [u8; MAX_SYSEX_FRAME],
}

impl<S: ConfigSink> ConfigTransport<S> {
    fn new(sink: S) -> Self {
        ConfigTransport {
            sink,
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

        self.sink.write_frame(&self.frame_buf[..frame_len]).await
    }
}
