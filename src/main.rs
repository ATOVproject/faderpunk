#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod app;
mod apps;
// TODO: Remove drivers, put in driver implementation crate
mod drivers;
mod tasks;

use apps::{get_channels, run_app_by_id};
use async_button::{Button, ButtonConfig, ButtonEvent};
use defmt::info;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::{Executor, Spawner};
use embassy_futures::join::join;
use embassy_futures::select::select;
use embassy_rp::block::ImageDef;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{UART0, UART1, USB};
use embassy_rp::spi::{self, Phase, Polarity, Spi};
use embassy_rp::uart::{self, Async as UartAsync, Config as UartConfig, Uart, UartTx};
use embassy_rp::usb;
use embassy_rp::{
    bind_interrupts,
    i2c::{self, I2c},
    peripherals::{I2C1, PIO0},
    pio,
};
use embassy_sync::blocking_mutex::raw::{
    CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex,
};
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher};
use embassy_sync::signal::Signal;
use embassy_time::{Delay, Duration, Timer};
use portable_atomic::Ordering;

use heapless::Vec;
use tasks::max::MAX_VALUES_FADER;
use {defmt_rtt as _, panic_probe as _};

use static_cell::StaticCell;

use array_init::array_init;
use at24cx::{Address, At24Cx};
use sequential_storage::{
    cache::NoCache,
    map::{fetch_item, store_item},
};

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: ImageDef = ImageDef::secure_exe();

// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Phoenix 16"),
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
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

pub type XSender = Sender<'static, NoopRawMutex, (usize, XTxMsg), 128>;

#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum XTxMsg {
    ButtonDown,
    FaderChange,
}

static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static mut CORE1_STACK: Stack<131_072> = Stack::new();
pub static CHANS_X: [PubSubChannel<ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>; 16] =
    [const { PubSubChannel::new() }; 16];
// Collector channel on core 0
static CHAN_X_0: StaticCell<Channel<NoopRawMutex, (usize, XTxMsg), 128>> = StaticCell::new();
static CHAN_X_TX: Channel<CriticalSectionRawMutex, (usize, XTxMsg), 20> = Channel::new();
// pub static CANCEL_TASKS: Watch<CriticalSectionRawMutex, bool, 16> = Watch::new();

#[derive(Debug)]
enum SceneErr {
    AppSize,
    LayoutSize,
}

/// Scene creates a proper scene vec from just a slice of app ids
/// The vec contains a tupel of app ids and their corresponding size of chans
struct Scene {
    apps: Vec<(usize, usize), 16>,
}

impl Scene {
    fn try_from(app_ids: &[usize]) -> Result<Self, SceneErr> {
        if app_ids.len() > 16 {
            return Err(SceneErr::AppSize);
        }
        // Create vec of (app_id, size). Will remove invalid app ids
        let apps: Vec<(usize, usize), 16> = app_ids
            .iter()
            .filter_map(|&id| {
                if let Some(size) = get_channels(id) {
                    return Some((id, size));
                }
                None
            })
            .collect::<Vec<(usize, usize), 16>>();
        // Check if apps fit into the layout
        let count = apps.iter().copied().fold(0, |acc, (_, size)| acc + size);
        if count > 16 {
            return Err(SceneErr::LayoutSize);
        }
        Ok(Self { apps })
    }

    fn apps_iter(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.apps.iter().scan(0, |start_channel, &(app_id, size)| {
            let result = Some((app_id, *start_channel));
            *start_channel += size;
            result
        })
    }

    // Creates an array which returns the app's start channel for every channel
    fn channel_map(&self) -> [usize; 16] {
        let mut result = [0; 16];

        for (_, start_chan) in self.apps_iter() {
            let size = 16 - start_chan;
            let end = (start_chan + size).min(16);
            result[start_chan..end].fill(start_chan);
        }

        result
    }
}

// TODO: create config builder to create full 16 channel layout with various apps
// The app at some point needs access to the MAX to configure it. Maybe this can happen via
// CHANNEL?
// Builder config needs to be serializable to store in eeprom

// App slots
#[embassy_executor::task(pool_size = 16)]
async fn run_app(number: usize, start_channel: usize) {
    let runner = run_app_by_id(number, start_channel);
    // TODO: Like this the canceller receiver should be dropped and its slot will be freed
    // let mut canceller = CANCEL_TASKS.receiver().unwrap();
    // select(runner, canceller.changed()).await;
    runner.await;
}

// Cross core comms
#[embassy_executor::task]
async fn x_recv(
    publishers: [Publisher<'static, ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>; 16],
    channel_map: [usize; 16],
) {
    loop {
        let (chan, msg) = CHAN_X_TX.receive().await;
        let start_chan = channel_map[chan];
        let relative_index = chan.wrapping_sub(start_chan);
        publishers[start_chan].publish((relative_index, msg)).await;
    }
}

// #[embassy_executor::task]
// async fn read_clock(
//     max_port: Mode0Port<Spi<'_, SPI0, spi::Async>, Output<'_>, ThreadModeRawMutex>,
// ) {
//     let clock_port = max_port
//         .into_configured_port(ConfigMode7(
//             AVR::InternalRef,
//             ADCRANGE::Rg0_2v5,
//             NSAMPLES::Samples16,
//         ))
//         .await
//         .unwrap();
//     let mut counter = 0u16;
//     let mut now = Instant::now();
//     info!("STARTED READING VALUES");
//     loop {
//         let _val = clock_port.get_value().await.unwrap();
//         counter += 1;
//         // Timer::after_micros(500).await;
//         if counter == 1000 {
//             let later = Instant::now();
//             let duration = later.checked_duration_since(now).unwrap();
//             now = later;
//             counter = 0;
//             info!(
//                 "Read clock 1000 times within {} millis",
//                 duration.as_millis()
//             );
//         }
//     }
// }

// TODO: We can not exchange channels for others. We have to re-run this whole
// function (which is fine?)
fn setup_channels(spawner: Spawner, scene: Scene) {
    let publishers: [Publisher<'static, ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>; 16] =
        array_init(|i| CHANS_X[i].publisher().unwrap());

    for (app_id, start_chan) in scene.apps_iter() {
        // TODO: Use AtomicU16 to cancel tasks (break out when bit for channel is high)
        // We only replace ALL 16 channels at once
        // TODO: TO CANCEL, we can try to wrap the whole thing in select(), and the second
        // one cancels when an atomic is set
        // Apparently we need to use signals to cancel the tasks (can use the xCore
        // channel from above)
        // https://github.com/embassy-rs/embassy/blob/main/examples/rp/src/bin/orchestrate_tasks.rs
        spawner.spawn(run_app(app_id, start_chan)).unwrap();
    }

    let channel_map = scene.channel_map();

    spawner.spawn(x_recv(publishers, channel_map)).unwrap();
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

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
    let i2c1 = i2c::I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2c::Config::default());

    // MIDI
    let mut uart_config = UartConfig::default();
    // Classic MIDI baud rate
    uart_config.baudrate = 31250;
    // MIDI Thru
    let uart0: UartTx<'_, _, UartAsync> = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH2, uart_config);
    // MIDI In/Out
    let uart1 = Uart::new(
        p.UART1,
        p.PIN_8,
        p.PIN_9,
        Irqs,
        p.DMA_CH3,
        p.DMA_CH4,
        uart_config,
    );

    // Buttons
    let buttons = (
        p.PIN_6, p.PIN_7, p.PIN_38, p.PIN_32, p.PIN_33, p.PIN_34, p.PIN_35, p.PIN_36, p.PIN_23,
        p.PIN_24, p.PIN_25, p.PIN_29, p.PIN_30, p.PIN_31, p.PIN_37, p.PIN_28, p.PIN_4, p.PIN_5,
    );

    // TODO: how do we re-spawn things??
    // 1) Make sure we can "unspawn" stuff
    // 2) Save all available scenes somewhere (16)
    // 3) Save the current scene index somewhere
    // 4) Store all available scenes and current scene index in eeprom, then read that on reboot
    //    into ram
    // 5) On scene change, unspawn everything, change current scene, spawn everything again

    // TODO: This config comes from the eeprom. We need a Vec of app numbers
    // Also do a sanity check here before we pass it to the other core
    let scene = Scene::try_from(&[1; 16]).unwrap();

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| setup_channels(spawner, scene));
        },
    );

    let chan_x_0 = CHAN_X_0.init(Channel::new());

    // spawner.spawn(read_clock(ports.port17)).unwrap();

    tasks::max::start_max(
        &spawner,
        spi0,
        p.PIO0,
        mux_pins,
        p.PIN_17,
        chan_x_0.sender(),
    )
    .await;

    tasks::usb::start_usb(&spawner, p.USB).await;

    tasks::serial::start_uart(&spawner, uart0, uart1).await;

    // Disabled for now
    //tasks::leds::start_leds(&spawner, spi1).await;

    tasks::buttons::start_buttons(&spawner, buttons, chan_x_0.sender()).await;

    let mut eeprom = At24Cx::new(i2c1, Address(0, 0), 17, Delay);

    // These are the flash addresses in which the crate will operate.
    // The crate will not read, write or erase outside of this range.
    let flash_range = 0x1000..0x3000;
    // We need to give the crate a buffer to work with.
    // It must be big enough to serialize the biggest value of your storage type in,
    // rounded up to to word alignment of the flash. Some kinds of internal flash may require
    // this buffer to be aligned in RAM as well.
    let mut data_buffer = [0; 128];
    let mut i = 0_u8;

    let fut = async {
        loop {
            let msg = chan_x_0.receive().await;
            CHAN_X_TX.send(msg).await;
            // let mut chan_x_tx = CHAN_X_TX.lock().await;
            // loop {
            //     let mut i: usize = 0;
            //     if let Ok(msg) = chan_x_0.try_receive() {
            //         chan_x_tx.enqueue(msg).unwrap();
            //     }
            //     if chan_x_0.is_empty() {
            //         SIG_X_TX.signal(());
            //         info!("Sent {} commands", i);
            //         break;
            //     }
            //     i += 1;
            // }
        }
    };

    let fut2 = async {
        loop {
            Timer::after_secs(1).await;
            // let fader0 = MAX_VALUES_FADER[0].load(Ordering::Relaxed);
            // let fader1 = MAX_VALUES_FADER[1].load(Ordering::Relaxed);
            // let fader2 = MAX_VALUES_FADER[2].load(Ordering::Relaxed);
            // info!("FADER VALUES: {} {} {}", fader0, fader1, fader2);
        }
    };

    join(fut, fut2).await;

    // // These are the flash addresses in which the crate will operate.
    // // The crate will not read, write or erase outside of this range.
    // let flash_range = 0x1000..0x3000;
    // // We need to give the crate a buffer to work with.
    // // It must be big enough to serialize the biggest value of your storage type in,
    // // rounded up to to word alignment of the flash. Some kinds of internal flash may require
    // // this buffer to be aligned in RAM as well.
    // let mut data_buffer = [0; 128];

    // Now we store an item the flash with key 42.
    // Again we make sure we pass the correct key and value types, u8 and u32.
    // It is important to do this consistently.

    // store_item(
    //     &mut eeprom,
    //     flash_range.clone(),
    //     &mut NoCache::new(),
    //     &mut data_buffer,
    //     &42u8,
    //     &104729u32,
    // )
    // .await
    // .unwrap();

    // When we ask for key 42, we not get back a Some with the correct value
    //
    // let val = fetch_item::<u8, u32, _>(
    //     &mut eeprom,
    //     flash_range.clone(),
    //     &mut NoCache::new(),
    //     &mut data_buffer,
    //     &42,
    // )
    // .await
    // .unwrap()
    // .unwrap();
    //
    // info!("VAL IS {}", val);
    //
    // assert_eq!(val, 104729);
    //
    // info!("INITIALIZED");
}
