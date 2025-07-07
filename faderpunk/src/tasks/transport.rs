use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcAcmState};
use embassy_usb::class::midi::MidiClass;
use embassy_usb::class::web_usb::{Config as WebUsbConfig, State as WebUsbState, Url, WebUsb};
use embassy_usb::driver::Driver;
use embassy_usb::msos::{self, windows_version};
use embassy_usb::{Builder, Config as UsbConfig};

use embassy_rp::peripherals::{UART0, UART1};
use embassy_rp::uart::{Async, BufferedUart, UartTx};

use super::configure::start_webusb_loop;
use super::midi::start_midi_loops;

const USB_VENDOR_ID: u16 = 0xf569;
const USB_PRODUCT_ID: u16 = 0x1;
const USB_VENDOR_NAME: &str = "ATOV";
const USB_PRODUCT_NAME: &str = "Faderpunk";
const USB_INTERFACE_GUIDS: &[&str] = &["{3A8E7B0C-F569-4A21-9B7D-6E2C1F8A9D04}"];

pub const USB_MAX_PACKET_SIZE: u16 = 64;

pub struct WebEndpoints<'d, D: Driver<'d>> {
    write_ep: D::EndpointIn,
    read_ep: D::EndpointOut,
}

impl<'d, D: Driver<'d>> WebEndpoints<'d, D> {
    fn new(builder: &mut Builder<'d, D>, config: &'d WebUsbConfig<'d>) -> Self {
        let mut func = builder.function(0xFF, 0x00, 0x00);
        let mut iface = func.interface();
        let mut alt = iface.alt_setting(0xFF, 0x00, 0x00, None);

        let write_ep = alt.endpoint_bulk_in(config.max_packet_size);
        let read_ep = alt.endpoint_bulk_out(config.max_packet_size);

        WebEndpoints { write_ep, read_ep }
    }

    pub fn split(self) -> (D::EndpointIn, D::EndpointOut) {
        (self.write_ep, self.read_ep)
    }
}

pub async fn start_transports(
    spawner: &Spawner,
    usb_driver: usb::Driver<'static, USB>,
    uart0: UartTx<'static, UART0, Async>,
    uart1: BufferedUart<'static, UART1>,
) {
    spawner
        .spawn(run_transports(usb_driver, uart0, uart1))
        .unwrap();
}

#[embassy_executor::task]
async fn run_transports(
    usb_driver: usb::Driver<'static, USB>,
    uart0: UartTx<'static, UART0, Async>,
    uart1: BufferedUart<'static, UART1>,
) {
    let mut usb_config = UsbConfig::new(USB_VENDOR_ID, USB_PRODUCT_ID);
    usb_config.manufacturer = Some(USB_VENDOR_NAME);
    usb_config.product = Some(USB_PRODUCT_NAME);
    // 0x0 (Major) | 0x1 (Minor) | 0x0 (Patch)
    usb_config.device_release = 0x010;
    usb_config.max_power = 500;
    usb_config.max_packet_size_0 = USB_MAX_PACKET_SIZE as u8;
    usb_config.device_class = 0xEF;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;
    usb_config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 128];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let webusb_config = WebUsbConfig {
        max_packet_size: USB_MAX_PACKET_SIZE,
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
        &mut msos_descriptor,
        &mut control_buf,
    );

    // Add msos descriptors for windows compatibility
    usb_builder.msos_descriptor(windows_version::WIN8_1, 0);
    usb_builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    usb_builder.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
        "DeviceInterfaceGUIDs",
        msos::PropertyData::RegMultiSz(USB_INTERFACE_GUIDS),
    ));

    // Create classes on the builder (WebUSB just needs some setup, but doesn't return anything)
    WebUsb::configure(&mut usb_builder, &mut webusb_state, &webusb_config);
    let webusb = WebEndpoints::new(&mut usb_builder, &webusb_config);

    // Create classes on the builder.
    let usb_midi = MidiClass::new(&mut usb_builder, 1, 1, USB_MAX_PACKET_SIZE);

    // Create USB logger
    let usb_logger = CdcAcmClass::new(&mut usb_builder, &mut logger_state, USB_MAX_PACKET_SIZE);
    let log_fut = embassy_usb_logger::with_class!(1024, log::LevelFilter::Info, usb_logger);

    let mut usb = usb_builder.build();

    // TODO: Can/should this be a task?
    // Maybe make all the other futs a task, then return midi_fut from here
    let midi_fut = start_midi_loops(usb_midi, uart0, uart1);
    let webusb_fut = start_webusb_loop(webusb);

    join4(usb.run(), log_fut, midi_fut, webusb_fut).await;
}
