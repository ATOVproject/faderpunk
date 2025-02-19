#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod app;
mod apps;
// TODO: Remove drivers, put in driver implementation crate
mod tasks;

use apps::{get_channels, run_app_by_id};
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
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};
use embassy_time::{Delay, Duration, Timer};
use midi2::channel_voice1::ChannelVoice1;
use portable_atomic::{AtomicBool, Ordering};

use heapless::Vec;
use tasks::leds::LedsAction;
use tasks::max::{MaxConfig, MAX_VALUES_FADER};
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
    embassy_rp::binary_info::rp_program_name!(c"Fader Punk"),
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

pub type XTxSender = Sender<'static, NoopRawMutex, (usize, XTxMsg), 128>;

/// Messages from core 0 to core 1
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum XTxMsg {
    ButtonDown,
    FaderChange,
}

/// Messages from core 1 to core 0
#[derive(Clone)]
pub enum XRxMsg {
    SetLed(LedsAction),
    MaxPortReconfigure(MaxConfig),
    MidiMessage(ChannelVoice1<[u8; 3]>),
}

static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static mut CORE1_STACK: Stack<131_072> = Stack::new();
pub static WATCH_SCENE_SET: Watch<CriticalSectionRawMutex, [usize; 16], 18> = Watch::new();
pub static CHANS_X: [PubSubChannel<ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>; 16] =
    [const { PubSubChannel::new() }; 16];
/// Collector channel on core 0
static CHAN_X_0: StaticCell<Channel<NoopRawMutex, (usize, XTxMsg), 128>> = StaticCell::new();
/// Collector channel on core 1
static CHAN_X_1: StaticCell<Channel<NoopRawMutex, (usize, XRxMsg), 128>> = StaticCell::new();
/// Channel from core 0 to core 1
static CHAN_X_TX: Channel<CriticalSectionRawMutex, (usize, XTxMsg), 64> = Channel::new();
/// Channel from core 1 to core 0
static CHAN_X_RX: Channel<CriticalSectionRawMutex, (usize, XRxMsg), 64> = Channel::new();
/// Channel for sending messages to the MAX
static CHAN_MAX: StaticCell<Channel<NoopRawMutex, (usize, MaxConfig), 64>> = StaticCell::new();
/// Channel for sending messages to the MIDI bus
static CHAN_MIDI: StaticCell<Channel<NoopRawMutex, (usize, ChannelVoice1<[u8; 3]>), 64>> =
    StaticCell::new();
/// Channel for sending messages to the LEDs
static CHAN_LEDS: StaticCell<Channel<NoopRawMutex, (usize, LedsAction), 64>> = StaticCell::new();
/// Tasks (apps) that are currently running (number 17 is the publisher task)
static CORE1_TASKS: [AtomicBool; 17] = [const { AtomicBool::new(false) }; 17];

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
async fn run_app(
    number: usize,
    start_channel: usize,
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
) {
    // INFO: This _should_ be properly dropped when task ends
    let mut cancel_receiver = WATCH_SCENE_SET.receiver().unwrap();
    // FIXME: Is the first value always new?
    let _ = cancel_receiver.changed().await;

    let run_app_fut = async {
        CORE1_TASKS[start_channel].store(true, Ordering::Relaxed);
        run_app_by_id(number, start_channel, sender).await;
    };

    select(run_app_fut, cancel_receiver.changed()).await;
    CORE1_TASKS[start_channel].store(false, Ordering::Relaxed);
    info!("App {} on channel {} stopped", number, start_channel)
}

// Cross core comms
#[embassy_executor::task]
async fn x_tx(channel_map: [usize; 16]) {
    // INFO: This _should_ be properly dropped when task ends
    let mut cancel_receiver = WATCH_SCENE_SET.receiver().unwrap();
    // FIXME: Is the first value always new?
    let _ = cancel_receiver.changed().await;
    let x_tx_fut = async {
        // INFO: These _should_ all be properly dropped when task ends
        let publishers: [Publisher<'static, ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>; 16] =
            array_init(|i| CHANS_X[i].publisher().unwrap());
        CORE1_TASKS[16].store(true, Ordering::Relaxed);
        loop {
            let (chan, msg) = CHAN_X_TX.receive().await;
            let start_chan = channel_map[chan];
            let relative_index = chan.wrapping_sub(start_chan);
            publishers[start_chan].publish((relative_index, msg)).await;
        }
    };

    select(x_tx_fut, cancel_receiver.changed()).await;
    CORE1_TASKS[16].store(false, Ordering::Relaxed);
}

#[embassy_executor::task]
async fn x_rx(receiver: Receiver<'static, NoopRawMutex, (usize, XRxMsg), 128>) {
    loop {
        let msg = receiver.receive().await;
        CHAN_X_RX.send(msg).await;
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

// fn cancel_tasks()

#[embassy_executor::task]
async fn main_core1(spawner: Spawner) {
    let chan_x_1 = CHAN_X_1.init(Channel::new());
    spawner.spawn(x_rx(chan_x_1.receiver())).unwrap();

    let mut receiver_scene = WATCH_SCENE_SET.receiver().unwrap();

    loop {
        let scene_arr = receiver_scene.changed().await;

        // Check if all tasks are properly exited
        loop {
            if CORE1_TASKS.iter().all(|val| !val.load(Ordering::Relaxed)) {
                break;
            }
            // yield to give apps time to close
            Timer::after_millis(5).await;
        }

        let scene = Scene::try_from(&scene_arr).unwrap();
        let channel_map = scene.channel_map();
        spawner
            // INFO: The next two _should_ be dropped properly when the task exits
            .spawn(x_tx(channel_map))
            .unwrap();
        for (app_id, start_chan) in scene.apps_iter() {
            spawner
                .spawn(run_app(
                    app_id,
                    start_chan,
                    // INFO: This _should_ be dropped properly when the task exits
                    chan_x_1.sender(),
                ))
                .unwrap();
        }
    }
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

    let chan_x_0 = CHAN_X_0.init(Channel::new());
    let chan_max = CHAN_MAX.init(Channel::new());
    let chan_midi = CHAN_MIDI.init(Channel::new());
    let chan_leds = CHAN_LEDS.init(Channel::new());

    // spawner.spawn(read_clock(ports.port17)).unwrap();

    tasks::max::start_max(
        &spawner,
        spi0,
        p.PIO0,
        mux_pins,
        p.PIN_17,
        chan_x_0.sender(),
        chan_max.receiver(),
    )
    .await;

    tasks::transport::start_transports(&spawner, p.USB, uart0, uart1, chan_midi.receiver()).await;

    tasks::leds::start_leds(&spawner, spi1, chan_leds.receiver()).await;

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

    let scene_sender = WATCH_SCENE_SET.sender();

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
            let (chan, msg) = CHAN_X_RX.receive().await;
            match msg {
                XRxMsg::MaxPortReconfigure(max_config) => chan_max.send((chan, max_config)).await,
                XRxMsg::SetLed(action) => chan_leds.send((chan, action)).await,
                XRxMsg::MidiMessage(midi_msg) => chan_midi.send((chan, midi_msg)).await,
            }

            // FIXME: Next steps: create a channel for each component (LEDs, MAX, MIDI) and then
            // listen on CHAN_X_RX on core 0, then send a message to the appropriate channels
        }
    };

    Timer::after_millis(100).await;

    scene_sender.send([1; 16]);

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
