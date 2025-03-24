use cobs::try_encode;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, Endpoint as UsbEndpoint, In, Out};
use embassy_usb::driver::{Endpoint, EndpointIn, EndpointOut};
use heapless::Vec;
use postcard::to_vec;

use config::ConfigMsgOut;

use crate::apps::{get_config, REGISTERED_APP_IDS};

use super::transport::WebEndpoints;

// TODO: We need to make this bigger for lots of params
const MAX_PAYLOAD_SIZE: usize = 256;
// NOTE: cobs needs max 1 byte for every 254 bytes of payload
// cobs (2) + delimiter (1)
const COBS_BYTES: usize = 3;
// length (2)
const PROTOCOL_BYTES: usize = 2;
/// Delimiter byte used for COBS framing
const FRAME_DELIMITER: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    BufferTooSmall,
    MessageTooLarge,
    DecodingError,
    EncodingError,
    InvalidMessageType,
    IncompleteMessage,
    TransmissionError,
    CorruptedMessage,
}

pub async fn start_webusb_loop<'a>(webusb: WebEndpoints<'a, Driver<'a, USB>>) {
    let mut proto = ConfigProtocol::new(webusb);
    let app_list = REGISTERED_APP_IDS.map(get_config);

    proto.wait_enabled().await;
    loop {
        // Test: send some app config to parse on the client side
        proto.read_msg().await.unwrap();
        proto
            .send_msg(ConfigMsgOut::AppList(&app_list))
            .await
            .unwrap();
    }
}

struct ConfigProtocol<'a> {
    send_buf: [u8; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
    recv_buf: [u8; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
    webusb_tx: UsbEndpoint<'a, USB, In>,
    webusb_rx: UsbEndpoint<'a, USB, Out>,
}

impl<'a> ConfigProtocol<'a> {
    fn new(webusb: WebEndpoints<'a, Driver<'a, USB>>) -> Self {
        let (webusb_tx, webusb_rx) = webusb.split();
        ConfigProtocol {
            send_buf: [0; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
            recv_buf: [0; MAX_PAYLOAD_SIZE + COBS_BYTES + PROTOCOL_BYTES],
            webusb_rx,
            webusb_tx,
        }
    }
    async fn wait_enabled(&mut self) {
        self.webusb_tx.wait_enabled().await;
        self.webusb_rx.wait_enabled().await;
    }
    async fn read_msg(&mut self) -> Result<(), ProtocolError> {
        let mut buf = [0; 64];
        self.webusb_rx
            .read(&mut buf)
            .await
            .map_err(|_| ProtocolError::TransmissionError)?;
        Ok(())
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
