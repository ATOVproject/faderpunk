#![no_std]
#![no_main]

#[macro_use]
pub mod macros;

mod app;
mod apps;
pub mod scene;
pub mod storage;
mod tasks;

use defmt::info;
use embassy_executor::{Executor, Spawner};
use embassy_futures::select::select;
use embassy_rp::clocks::ClockConfig;
use embassy_rp::config::Config;
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{UART0, UART1, USB};
use embassy_rp::spi::{self, Spi};
use embassy_rp::uart::{self, Async as UartAsync, BufferedUart, Config as UartConfig, UartTx};
use embassy_rp::usb;
use embassy_rp::{
    bind_interrupts, i2c,
    peripherals::{I2C1, PIO0},
    pio,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::watch::Watch;
use embassy_time::{Delay, Timer};
use midly::live::LiveEvent;
use portable_atomic::{AtomicBool, Ordering};

use tasks::max::{MaxCmd, MAX_CHANNEL};
use tasks::midi::MIDI_CHANNEL;
use {defmt_rtt as _, panic_probe as _};

use static_cell::StaticCell;

use at24cx::{Address, At24Cx};

use apps::run_app_by_id;
use config::{ClockSrc, GlobalConfig};

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
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    UART0_IRQ => uart::InterruptHandler<UART0>;
    UART1_IRQ => uart::BufferedInterruptHandler<UART1>;
});

static mut CORE1_STACK: Stack<131_072> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

// TODO: Move all of the message stuff to own file
/// Messages from core 0 to core 1
#[derive(Clone)]
pub enum HardwareEvent {
    ButtonDown(usize),
    ButtonUp(usize),
    FaderChange(usize),
    MidiMsg(LiveEvent<'static>),
}

/// Messages from core 1 to core 0
pub enum HardwareCmd {
    MaxCmd(usize, MaxCmd),
    MidiCmd(LiveEvent<'static>),
}

pub const CMD_CHANNEL_SIZE: usize = 16;
pub const EVENT_PUBSUB_SIZE: usize = 64;

// TODO: Adjust number of receivers accordingly (we need at least 18 for layout + x), then also
// mention all uses
pub static CONFIG_CHANGE_WATCH: Watch<CriticalSectionRawMutex, GlobalConfig, 26> =
    Watch::new_with(GlobalConfig::new());
pub static CLOCK_WATCH: Watch<CriticalSectionRawMutex, bool, 16> = Watch::new();

pub type EventPubSubChannel =
    PubSubChannel<CriticalSectionRawMutex, HardwareEvent, EVENT_PUBSUB_SIZE, 32, 21>;
pub static EVENT_PUBSUB: EventPubSubChannel = PubSubChannel::new();
pub static CMD_CHANNEL: Channel<CriticalSectionRawMutex, HardwareCmd, CMD_CHANNEL_SIZE> =
    Channel::new();
pub type CmdSender = Sender<'static, CriticalSectionRawMutex, HardwareCmd, CMD_CHANNEL_SIZE>;

/// Tasks (apps) that are currently running (number 17 is the publisher task)
static CORE1_TASKS: [AtomicBool; 17] = [const { AtomicBool::new(false) }; 17];

/// MIDI buffers (RX and TX)
static BUF_UART1_RX: StaticCell<[u8; 64]> = StaticCell::new();
static BUF_UART1_TX: StaticCell<[u8; 64]> = StaticCell::new();

// App slots
#[embassy_executor::task(pool_size = 16)]
async fn run_app(number: usize, start_channel: usize) {
    // INFO: This _should_ be properly dropped when task ends
    let mut cancel_receiver = CONFIG_CHANGE_WATCH.receiver().unwrap();
    // TODO: Is the first value always new?
    let _ = cancel_receiver.changed().await;

    let run_app_fut = async {
        CORE1_TASKS[start_channel].store(true, Ordering::Relaxed);
        run_app_by_id(number, start_channel).await;
    };

    select(run_app_fut, cancel_receiver.changed()).await;
    CORE1_TASKS[start_channel].store(false, Ordering::Relaxed);
    info!("App {} on channel {} stopped", number, start_channel)
}

#[embassy_executor::task]
async fn main_core1(spawner: Spawner) {
    let mut receiver_config = CONFIG_CHANGE_WATCH.receiver().unwrap();

    loop {
        let config = receiver_config.changed().await;

        // Check if all tasks are properly exited
        loop {
            if CORE1_TASKS.iter().all(|val| !val.load(Ordering::Relaxed)) {
                break;
            }
            // yield to give apps time to close
            Timer::after_millis(5).await;
        }

        for &(app_id, start_chan) in config.layout.iter() {
            spawner.spawn(run_app(app_id, start_chan)).unwrap();
            Timer::after_millis(20).await;
        }
    }
}

#[embassy_executor::task]
async fn hardware_cmd_router() {
    loop {
        match CMD_CHANNEL.receive().await {
            HardwareCmd::MaxCmd(channel, max_cmd) => MAX_CHANNEL.send((channel, max_cmd)).await,
            HardwareCmd::MidiCmd(live_event) => MIDI_CHANNEL.send(live_event).await,
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Overclock to 250Mhz
    let mut clock_config = ClockConfig::crystal(12_000_000);
    if let Some(ref mut xosc) = clock_config.xosc {
        if let Some(ref mut sys_pll) = xosc.sys_pll {
            //  TODO: Add calculation (post_div2 is 2)
            // Changed from 5 to 3
            sys_pll.post_div1 = 3;
        }
    }

    let mut config = Config::default();
    config.clocks = clock_config;

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

    // I2C1 (EEPROM)
    let mut i2c1_config = i2c::Config::default();
    i2c1_config.frequency = 1_000_000;
    let i2c1 = i2c::I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2c1_config);

    // MIDI
    let mut uart_config = UartConfig::default();
    // Classic MIDI baud rate
    uart_config.baudrate = 31250;
    // MIDI Thru
    let uart0: UartTx<'_, _, UartAsync> = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH2, uart_config);
    // MIDI In/Out
    let uart1_tx_buffer = BUF_UART1_TX.init([0; 64]);
    let uart1_rx_buffer = BUF_UART1_RX.init([0; 64]);
    let uart1 = BufferedUart::new(
        p.UART1,
        Irqs,
        p.PIN_8,
        p.PIN_9,
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

    // EEPROM
    let eeprom = At24Cx::new(i2c1, Address(0, 0), 17, Delay);

    // AUX inputs
    let aux_inputs = (p.PIN_1, p.PIN_2, p.PIN_3);

    tasks::max::start_max(&spawner, spi0, p.PIO0, mux_pins, p.PIN_17).await;

    tasks::transport::start_transports(&spawner, usb_driver, uart0, uart1).await;

    tasks::leds::start_leds(&spawner, spi1).await;

    tasks::buttons::start_buttons(&spawner, buttons).await;

    tasks::clock::start_clock(&spawner, aux_inputs).await;

    tasks::eeprom::start_eeprom(&spawner, eeprom).await;

    spawner.spawn(hardware_cmd_router()).unwrap();

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

    Timer::after_millis(100).await;

    // TODO: Get this from eeprom
    let mut config = GlobalConfig::default();
    config.clock_src = ClockSrc::MidiIn;
    config.reset_src = ClockSrc::MidiIn;
    // config.layout = Vec::from_slice(&[(3, 0), (3, 1)]).unwrap();

    config_sender.send(config);
}
