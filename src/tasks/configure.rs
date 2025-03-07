use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, Endpoint as UsbEndpoint, Out};
use embassy_usb::driver::{Endpoint, EndpointIn, EndpointOut};

use super::transport::WebEndpoints;

pub async fn start_webusb_loop<'a>(webusb: WebEndpoints<'a, Driver<'a, USB>>) {
    let (mut webusb_tx, mut webusb_rx) = webusb.split();

    webusb_rx.wait_enabled().await;
    webusb_tx.wait_enabled().await;
    loop {
        let data = read_msg(&mut webusb_rx).await;
        // FIXME: NEXT
        // 1) Implement protocol to be able to read over many "pages" (Start byte/stop byte?)
        // 2) Parse message from buffer into ConfigureMessage (use https://docs.rs/postcard-bindgen/latest/postcard_bindgen/)
        // 3) React appropriately. First: Serialize all params and send as message
    }
}

async fn read_msg(rx: &mut UsbEndpoint<'_, USB, Out>) -> Vec<u8, 256> {
    let mut frame = [0u8; 64];
    // TODO: We need to make this buffer bigger for lots of params
    let mut buf: Vec<u8, 256> = Vec::new();

    loop {
        let len = rx.read(&mut frame).await.unwrap();
        buf.extend_from_slice(&frame[..len]).unwrap();
        // TODO: Here we assume that frames that are 64 bits exactly are incomplete messages
        // That's definitely not the case especially as we can't control the size
        if len < frame.len() {
            break;
        }
    }

    buf
}

// // Wait until the device's endpoints are enabled.
// pub async fn wait_connected(&mut self) {
//     self.read_ep.wait_enabled().await
// }
//
// // Echo data back to the host.
// pub async fn echo(&mut self) {
//     let mut buf = [0; 64];
//     loop {
//         let n = self.read_ep.read(&mut buf).await.unwrap();
//         let data = &buf[..n];
//         info!("Data read: {:x}", data);
//         self.write_ep.write(data).await.unwrap();
//     }
// }
