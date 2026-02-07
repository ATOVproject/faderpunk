use cobs::{decode_in_place, try_encode};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, Endpoint as UsbEndpoint, In, Out};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Duration};
use embassy_usb::driver::{Endpoint, EndpointIn, EndpointOut};
use heapless::Vec;
use postcard::{from_bytes, to_vec};

use libfp::{ConfigMsgIn, ConfigMsgOut, Value, APP_MAX_PARAMS, GLOBAL_CHANNELS};

use crate::apps::{get_channels, get_config, REGISTERED_APP_IDS};
use crate::layout::LAYOUT_WATCH;
use crate::storage::factory_reset;
use crate::tasks::global_config::{get_global_config, GLOBAL_CONFIG_WATCH};

use super::transport::{WebEndpoints, USB_MAX_PACKET_SIZE};

const MAX_PAYLOAD_SIZE: usize = 512;
// cobs needs max 1 byte for every 254 bytes of payload
// cobs (2) + delimiter (1)
const COBS_BYTES: usize = 3;
// length (2)
const PROTOCOL_BYTES: usize = 2;
/// Delimiter byte used for COBS framing
const FRAME_DELIMITER: u8 = 0;
/// Multi-packet message timeout in ms
const MULTI_PACKET_TIMEOUT_MS: u64 = 100;

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
pub enum ProtocolError {
    BufferTooSmall,
    MessageTooLarge,
    DecodingError,
    EncodingError,
    TransmissionError,
    CorruptedMessage,
    Timeout,
}

pub async fn start_webusb_loop<'a>(webusb: WebEndpoints<'a, Driver<'a, USB>>) {
    let mut proto = ConfigProtocol::new(webusb);
    let mut layout_receiver = LAYOUT_WATCH.receiver().unwrap();
    proto.wait_enabled().await;
    loop {
        // Test: send some app config to parse on the client side
        let msg = proto.read_msg().await.unwrap();
        match msg {
            ConfigMsgIn::Ping => {
                proto.send_msg(ConfigMsgOut::Pong).await.unwrap();
            }
            ConfigMsgIn::GetAllApps => {
                let configs = REGISTERED_APP_IDS.map(get_config);
                proto
                    .send_msg(ConfigMsgOut::BatchMsgStart(configs.len()))
                    .await
                    .unwrap();
                for (app_id, channels, config_meta) in configs.into_iter().flatten() {
                    proto
                        .send_msg(ConfigMsgOut::AppConfig(app_id, channels, config_meta))
                        .await
                        .unwrap();
                }
                proto.send_msg(ConfigMsgOut::BatchMsgEnd).await.unwrap();
            }
            ConfigMsgIn::GetLayout => {
                let layout = layout_receiver.get().await;
                proto.send_msg(ConfigMsgOut::Layout(layout)).await.unwrap();
            }
            ConfigMsgIn::GetGlobalConfig => {
                let config = get_global_config();
                proto
                    .send_msg(ConfigMsgOut::GlobalConfig(config))
                    .await
                    .unwrap();
            }
            ConfigMsgIn::GetAppParams { layout_id } => {
                APP_PARAM_SIGNALS[layout_id as usize].signal(AppParamCmd::RequestParamValues);
                if let Ok((res_layout_id, values)) =
                    with_timeout(Duration::from_secs(1), APP_PARAM_CHANNEL.receive()).await
                {
                    proto
                        .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                        .await
                        .unwrap();
                }
            }
            ConfigMsgIn::SetAppParams { layout_id, values } => {
                APP_PARAM_SIGNALS[layout_id as usize].signal(AppParamCmd::SetAppParams { values });
                if let Ok((res_layout_id, values)) =
                    with_timeout(Duration::from_secs(1), APP_PARAM_CHANNEL.receive()).await
                {
                    proto
                        .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                        .await
                        .unwrap();
                }
            }
            ConfigMsgIn::GetAllAppParams => {
                let layout = layout_receiver.get().await;
                let layout_ids = layout.get_layout_ids();
                let app_count = layout_ids.len();

                proto
                    .send_msg(ConfigMsgOut::BatchMsgStart(app_count))
                    .await
                    .unwrap();

                if app_count > 0 {
                    for id in layout_ids {
                        APP_PARAM_SIGNALS[id as usize].signal(AppParamCmd::RequestParamValues);
                    }
                    let receiver = async {
                        for _ in 0..app_count {
                            let (res_layout_id, values) = APP_PARAM_CHANNEL.receive().await;
                            proto
                                .send_msg(ConfigMsgOut::AppState(res_layout_id, &values))
                                .await
                                .unwrap();
                        }
                    };

                    with_timeout(Duration::from_secs(1), receiver).await.ok();
                }

                proto.send_msg(ConfigMsgOut::BatchMsgEnd).await.unwrap();
            }
            ConfigMsgIn::SetGlobalConfig(mut global_config) => {
                global_config.validate();
                let sender = GLOBAL_CONFIG_WATCH.sender();
                sender.send(global_config);
            }
            ConfigMsgIn::SetLayout(mut layout) => {
                layout.validate(get_channels);
                let sender = LAYOUT_WATCH.sender();
                proto
                    .send_msg(ConfigMsgOut::Layout(layout.clone()))
                    .await
                    .unwrap();
                sender.send(layout);
            }
            ConfigMsgIn::FactoryReset => {
                factory_reset().await;
            }
        }
    }
}

struct ConfigProtocol<'a> {
    send_buf: [u8; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
    webusb_tx: UsbEndpoint<'a, USB, In>,
    webusb_rx: UsbEndpoint<'a, USB, Out>,
}

impl<'a> ConfigProtocol<'a> {
    fn new(webusb: WebEndpoints<'a, Driver<'a, USB>>) -> Self {
        let (webusb_tx, webusb_rx) = webusb.split();
        ConfigProtocol {
            send_buf: [0; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
            webusb_rx,
            webusb_tx,
        }
    }
    async fn wait_enabled(&mut self) {
        self.webusb_tx.wait_enabled().await;
        self.webusb_rx.wait_enabled().await;
    }
    async fn read_remaining_packets(
        &mut self,
        buf: &mut [u8],
        mut cursor: usize,
    ) -> Result<ConfigMsgIn, ProtocolError> {
        loop {
            if cursor + USB_MAX_PACKET_SIZE as usize > buf.len() {
                return Err(ProtocolError::MessageTooLarge);
            }

            let bytes_read = self
                .webusb_rx
                .read(&mut buf[cursor..cursor + USB_MAX_PACKET_SIZE as usize])
                .await
                .map_err(|_| ProtocolError::TransmissionError)?;

            // Check if the message is complete
            if let Some(end) = buf[cursor..cursor + bytes_read]
                .iter()
                .position(|&x| x == FRAME_DELIMITER)
            {
                return self.process_message(&mut buf[..cursor + end]);
            }

            cursor += bytes_read;
        }
    }
    fn process_message(&self, buf: &mut [u8]) -> Result<ConfigMsgIn, ProtocolError> {
        let rx_size = decode_in_place(buf).map_err(|_| ProtocolError::DecodingError)?;

        let payload_len = ((buf[0] as usize) << 8) | buf[1] as usize;
        if payload_len != rx_size - 2 {
            return Err(ProtocolError::CorruptedMessage);
        }

        let msg = from_bytes(&buf[2..rx_size]).map_err(|_| ProtocolError::DecodingError)?;
        Ok(msg)
    }
    async fn read_msg(&mut self) -> Result<ConfigMsgIn, ProtocolError> {
        let mut buf = [0; MAX_PAYLOAD_SIZE + PROTOCOL_BYTES + COBS_BYTES];

        let bytes_read = self
            .webusb_rx
            .read(&mut buf[0..USB_MAX_PACKET_SIZE as usize])
            .await
            .map_err(|_| ProtocolError::TransmissionError)?;

        if bytes_read == 0 {
            return Err(ProtocolError::TransmissionError);
        }

        // Check if the message is already complete
        if let Some(end) = buf[..bytes_read].iter().position(|&x| x == FRAME_DELIMITER) {
            return self.process_message(&mut buf[..end]);
        }

        with_timeout(
            Duration::from_millis(MULTI_PACKET_TIMEOUT_MS),
            self.read_remaining_packets(&mut buf, bytes_read),
        )
        .await
        .map_err(|_| ProtocolError::Timeout)?
    }
    async fn send_msg(&mut self, msg: ConfigMsgOut<'_>) -> Result<(), ProtocolError> {
        let mut out: Vec<u8, { MAX_PAYLOAD_SIZE + PROTOCOL_BYTES }> =
            to_vec(&msg).map_err(|_| ProtocolError::EncodingError)?;
        let payload_len = out.len();

        out.insert(0, ((payload_len >> 8) & 0xFF) as u8)
            .map_err(|_| ProtocolError::MessageTooLarge)?;
        out.insert(1, (payload_len & 0xFF) as u8)
            .map_err(|_| ProtocolError::MessageTooLarge)?;

        let total_len = payload_len + PROTOCOL_BYTES;
        let tx_size = try_encode(&out[..total_len], self.send_buf.as_mut())
            .map_err(|_| ProtocolError::BufferTooSmall)?;

        self.send_buf[tx_size] = FRAME_DELIMITER;
        for chunk in self.send_buf[..tx_size + 1].chunks(64) {
            self.webusb_tx
                .write(chunk)
                .await
                .map_err(|_| ProtocolError::TransmissionError)?;
        }

        Ok(())
    }
}
