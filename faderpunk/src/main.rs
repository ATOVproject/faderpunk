#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod app;
mod apps;
mod events;
mod layout;
mod storage;
mod tasks;

use core::sync::atomic::Ordering;

use embassy_executor::{Executor, Spawner};
use embassy_rp::clocks::{ClockConfig, CoreVoltage};
use embassy_rp::config::Config;
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{UART0, UART1, USB};
use embassy_rp::spi::{self, Spi};
use embassy_rp::uart::{self, Async as UartAsync, BufferedUart, Config as UartConfig, UartTx};
use embassy_rp::{
    bind_interrupts, i2c,
    peripherals::{I2C0, I2C1, PIO0},
    pio,
};
use embassy_rp::{i2c_slave, usb};
use fm24v10::{Address, Fm24v10};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use libfp::I2C_ADDRESS;

use events::{CONFIG_CHANGE_WATCH, LAYOUT_CHANGE_WATCH};
use layout::{LayoutManager, LAYOUT_MANAGER};
use storage::{load_calibration_data, load_global_config, load_layout};
use tasks::{buttons::BUTTON_PRESSED, fram::MAX_DATA_LEN, max::MAX_CHANNEL, midi::MIDI_CHANNEL};

// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Faderpunk"),
    embassy_rp::binary_info::rp_program_description!(
        c"From ember's grip, a fader's rise, In ancient garb, under modern skies. A phoenix's touch, in keys it lays, A melody bold, through time's maze."
    ),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(struct Irqs {
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    UART0_IRQ => uart::InterruptHandler<UART0>;
    UART1_IRQ => uart::BufferedInterruptHandler<UART1>;
});

static mut CORE1_STACK: Stack<131_072> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

/// MIDI buffers (RX and TX)
static BUF_UART1_RX: StaticCell<[u8; 64]> = StaticCell::new();
static BUF_UART1_TX: StaticCell<[u8; 64]> = StaticCell::new();

/// FRAM write buffer
static BUF_FRAM_WRITE: StaticCell<[u8; MAX_DATA_LEN]> = StaticCell::new();

#[embassy_executor::task]
async fn main_core1(spawner: Spawner) {
    let lm = LAYOUT_MANAGER.init(LayoutManager::new(spawner));
    let mut receiver = LAYOUT_CHANGE_WATCH.receiver().unwrap();
    loop {
        let layout = receiver.changed().await;
        lm.spawn_layout(layout).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Overclock to 250Mhz
    let mut config = Config::new(ClockConfig::system_freq(250_000_000).unwrap());
    config.clocks.core_voltage = CoreVoltage::V1_15;

    let p = embassy_rp::init(config);

    // SPI0 (MAX11300)
    let mut spi0_config = spi::Config::default();
    spi0_config.frequency = 20_000_000;
    let spi0 = Spi::new(
        p.SPI0,
        p.PIN_18,
        p.PIN_19,
        p.PIN_16,
        p.DMA_CH0,
        p.DMA_CH1,
        spi0_config,
    );
    let mux_pins = (p.PIN_12, p.PIN_13, p.PIN_14, p.PIN_15);

    // SPI1 (WS2812)
    let mut spi1_config = spi::Config::default();
    spi1_config.frequency = 3_800_000;
    let spi1 = Spi::new_txonly(p.SPI1, p.PIN_10, p.PIN_11, p.DMA_CH5, spi1_config);

    // I2C0 (external I2C)
    let mut i2c0_config = i2c_slave::Config::default();
    i2c0_config.addr = I2C_ADDRESS;
    let mut i2c0 = i2c_slave::I2cSlave::new(p.I2C0, p.PIN_21, p.PIN_20, Irqs, i2c0_config);

    // I2C1 (FRAM)
    let mut i2c1_config = i2c::Config::default();
    i2c1_config.frequency = 1_000_000;
    let i2c1 = i2c::I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2c1_config);

    // MIDI
    let mut uart_config = UartConfig::default();
    // Classic MIDI baud rate
    uart_config.baudrate = 31250;
    // MIDI Thru
    let uart0: UartTx<'_, UartAsync> = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH2, uart_config);
    // MIDI In/Out
    let uart1_tx_buffer = BUF_UART1_TX.init([0; 64]);
    let uart1_rx_buffer = BUF_UART1_RX.init([0; 64]);
    let uart1 = BufferedUart::new(
        p.UART1,
        p.PIN_8,
        p.PIN_9,
        Irqs,
        uart1_tx_buffer,
        uart1_rx_buffer,
        uart_config,
    );

    // USB
    let usb_driver = usb::Driver::new(p.USB, Irqs);

    // Buttons
    let buttons = (
        p.PIN_6, p.PIN_7, p.PIN_38, p.PIN_32, p.PIN_33, p.PIN_34, p.PIN_35, p.PIN_36, p.PIN_23,
        p.PIN_24, p.PIN_25, p.PIN_29, p.PIN_30, p.PIN_31, p.PIN_37, p.PIN_28, p.PIN_4, p.PIN_5,
    );

    // FRAM
    let write_buf = BUF_FRAM_WRITE.init([0; MAX_DATA_LEN]);
    let fram = Fm24v10::new(i2c1, Address(0, 0), write_buf);

    // AUX inputs
    let aux_inputs = (p.PIN_1, p.PIN_2, p.PIN_3);

    tasks::fram::start_fram(&spawner, fram).await;

    tasks::leds::start_leds(&spawner, spi1).await;

    tasks::buttons::start_buttons(&spawner, buttons).await;

    let calibration_data = load_calibration_data().await;

    tasks::max::start_max(&spawner, spi0, p.PIO0, mux_pins, p.PIN_17, calibration_data).await;

    // tasks::i2c::start_i2c(&spawner, i2c0).await;

    tasks::i2c::run_calibration(&mut i2c0).await;

    tasks::transport::start_transports(&spawner, usb_driver, uart0, uart1).await;

    tasks::clock::start_clock(&spawner, aux_inputs).await;

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(main_core1(spawner)).unwrap();
            });
        },
    );

    let config_sender = CONFIG_CHANGE_WATCH.sender();
    let layout_sender = LAYOUT_CHANGE_WATCH.sender();

    let global_config = load_global_config().await;
    let mut layout = load_layout().await;

    // Load calibration app if there is no calibration data or
    // when scene + shift are pressed during startup
    if calibration_data.is_none() || BUTTON_PRESSED[16].load(Ordering::Relaxed) {
        layout.0[0] = Some((255, 16));
    }

    // Initialize the device with the loaded config and layout
    config_sender.send(global_config);
    layout_sender.send(layout);
}
