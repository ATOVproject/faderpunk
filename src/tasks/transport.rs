use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcAcmState};
use embassy_usb::class::midi::MidiClass;
use embassy_usb::class::web_usb::{Config as WebUsbConfig, State as WebUsbState, Url, WebUsb};
use embassy_usb::driver::{Driver, Endpoint, EndpointIn, EndpointOut};
use embassy_usb::{Builder, Config as UsbConfig};

use embassy_rp::peripherals::{UART0, UART1};
use embassy_rp::uart::{Async, Uart, UartTx};

use crate::Irqs;

use super::midi::{start_midi_loops, XRxReceiver};

// This is a randomly generated GUID to allow clients on Windows to find our device
const DEVICE_INTERFACE_GUIDS: &[&str] = &["{AFB9A6FB-30BA-44BC-9232-806CFC875321}"];

struct WebEndpoints<'d, D: Driver<'d>> {
    write_ep: D::EndpointIn,
    read_ep: D::EndpointOut,
}

impl<'d, D: Driver<'d>> WebEndpoints<'d, D> {
    fn new(builder: &mut Builder<'d, D>, config: &'d WebUsbConfig<'d>) -> Self {
        let mut func = builder.function(0xff, 0x00, 0x00);
        let mut iface = func.interface();
        let mut alt = iface.alt_setting(0xff, 0x00, 0x00, None);

        let write_ep = alt.endpoint_bulk_in(config.max_packet_size);
        let read_ep = alt.endpoint_bulk_out(config.max_packet_size);

        WebEndpoints { write_ep, read_ep }
    }

    // Wait until the device's endpoints are enabled.
    async fn wait_connected(&mut self) {
        self.read_ep.wait_enabled().await
    }

    // Echo data back to the host.
    async fn echo(&mut self) {
        let mut buf = [0; 64];
        loop {
            let n = self.read_ep.read(&mut buf).await.unwrap();
            let data = &buf[..n];
            info!("Data read: {:x}", data);
            self.write_ep.write(data).await.unwrap();
        }
    }
}

pub async fn start_transports(
    spawner: &Spawner,
    usb0: USB,
    uart0: UartTx<'static, UART0, Async>,
    uart1: Uart<'static, UART1, Async>,
    x_rx: XRxReceiver,
) {
    spawner
        .spawn(run_transports(usb0, uart0, uart1, x_rx))
        .unwrap();
}

#[embassy_executor::task]
async fn run_transports(
    usb0: USB,
    uart0: UartTx<'static, UART0, Async>,
    uart1: Uart<'static, UART1, Async>,
    x_rx: XRxReceiver,
) {
    let usb_driver = usb::Driver::new(usb0, Irqs);
    let mut usb_config = UsbConfig::new(0xf569, 0x1);
    usb_config.manufacturer = Some("ATOV");
    usb_config.product = Some("Fader Punk");
    usb_config.serial_number = Some("12345678");
    // 0x0 (Major) | 0x1 (Minor) | 0x0 (Patch)
    usb_config.device_release = 0x010;
    usb_config.max_power = 500;
    // usb_config.max_packet_size_0 = 64;
    usb_config.device_class = 0xEF;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;
    usb_config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let webusb_config = WebUsbConfig {
        max_packet_size: 64,
        vendor_code: 1,
        landing_url: Some(Url::new("https://faderpunk.io")),
    };

    let mut webusb_state = WebUsbState::new();
    let mut logger_state = CdcAcmState::new();

    let mut usb_builder = Builder::new(
        usb_driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // Create classes on the builder (WebUSB just needs some setup, but doesn't return anything)
    WebUsb::configure(&mut usb_builder, &mut webusb_state, &webusb_config);
    // Create some USB bulk endpoints for testing.
    let mut endpoints = WebEndpoints::new(&mut usb_builder, &webusb_config);

    // Create classes on the builder.
    let usb_midi = MidiClass::new(&mut usb_builder, 1, 1, 64);

    // Create USB logger
    let usb_logger = CdcAcmClass::new(&mut usb_builder, &mut logger_state, 64);
    let log_fut = embassy_usb_logger::with_class!(1024, log::LevelFilter::Info, usb_logger);

    let mut usb = usb_builder.build();

    // TODO: Can/should this be a task?
    // Maybe make all the other futs a task, then return midi_fut from here
    let midi_fut = start_midi_loops(usb_midi, uart0, uart1, x_rx);

    // Do some WebUSB transfers.
    let webusb_fut = async {
        loop {
            endpoints.wait_connected().await;
            info!("WebUSB Connected");
            endpoints.echo().await;
        }
    };

    join4(usb.run(), midi_fut, webusb_fut, log_fut).await;
}
