#![no_std]
#![no_main]

use core::ptr::addr_of_mut;

use array_init::array_init;
use at24cx::{Address, At24Cx};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher};
use embassy_sync::signal::Signal;
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration, Timer};
use esp_backtrace as _;
use esp_hal::uart::UartTx;
use esp_hal::{
    i2c::master::{Config as I2cConfig, I2c},
    otg_fs::{
        asynch::{Config as UsbConfig, Driver as UsbDriver},
        Usb,
    },
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode as SpiMode,
    },
    system::{CpuControl, Stack},
    time::Rate,
    timer::{timg::TimerGroup, AnyTimer},
    uart::{Config as UartConfig, Uart},
    Async,
};
use esp_hal_embassy::Executor;
use esp_println::println;
use heapless::Vec;
use midi2::channel_voice1::ChannelVoice1;
use portable_atomic::{AtomicBool, Ordering};
use sequential_storage::{
    cache::NoCache,
    map::{fetch_item, store_item},
};
use static_cell::StaticCell;

use fader_punk::apps::{get_channels, run_app_by_id};
use fader_punk::tasks::{
    self,
    leds::LedsAction,
    max::{MaxConfig, MAX_VALUES_FADER},
};
use fader_punk::{
    config, XRxMsg, XTxMsg, CHANS_X, CHAN_LEDS, CHAN_MAX, CHAN_MIDI, CHAN_X_0, CHAN_X_1, CHAN_X_RX,
    CHAN_X_TX, CORE1_TASKS, GLOBAL_CONFIG, WATCH_SCENE_SET,
};

static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static mut CORE1_STACK: Stack<131_072> = Stack::new();
static I2C0_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> = StaticCell::new();
static EP_OUT_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

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
    println!("App {} on channel {} stopped", number, start_channel)
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
        // FIXME: Can we use NoopRawMutex?
        let publishers: [Publisher<'static, CriticalSectionRawMutex, (usize, XTxMsg), 64, 5, 1>;
            16] = array_init(|i| CHANS_X[i].publisher().unwrap());
        CORE1_TASKS[16].store(true, Ordering::Relaxed);
        loop {
            let (chan, msg) = CHAN_X_TX.receive().await;
            if chan == 16 {
                // INFO: Special channel 16 sends to all publishers
                for publisher in publishers.iter() {
                    publisher.publish((chan, msg)).await;
                }
            } else {
                let start_chan = channel_map[chan];
                let relative_index = chan.wrapping_sub(start_chan);
                publishers[start_chan].publish((relative_index, msg)).await;
            }
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

        let scene = Scene::try_from(scene_arr).unwrap();
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
    let p = esp_hal::init(esp_hal::Config::default());

    // FIXME: Which timer to use? Can we use SYS timer?
    let timg0 = TimerGroup::new(p.TIMG0);
    let timer0: AnyTimer = timg0.timer0.into();
    let timer1: AnyTimer = timg0.timer1.into();
    esp_hal_embassy::init([timer0, timer1]);

    let mut cpu_control = CpuControl::new(p.CPU_CTRL);

    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(CORE1_STACK) }, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|sp| {
                sp.spawn(main_core1(sp)).ok();
            });
        })
        .unwrap();

    // SPI2 (MAX11300)
    let mut spi_max = Spi::new(
        p.SPI2,
        SpiConfig::default().with_frequency(Rate::from_mhz(20)),
    )
    .unwrap()
    // Recommended SPI2 pins
    .with_sck(p.GPIO12)
    .with_mosi(p.GPIO11)
    .with_miso(p.GPIO13)
    .into_async();

    // Any pins are fine
    let mux_pins = (p.GPIO30, p.GPIO31, p.GPIO32, p.GPIO33);

    // SPI3 (WS2812)
    let mut spi_leds = Spi::new(
        p.SPI3,
        SpiConfig::default().with_frequency(Rate::from_khz(3_800)),
    )
    .unwrap()
    // Any pins are fine, really
    .with_sck(p.GPIO41)
    .with_mosi(p.GPIO40)
    .into_async();

    // I2C0 (EEPROM & Button expander)
    let i2c0 = I2c::new(p.I2C0, I2cConfig::default())
        .unwrap()
        .with_sda(p.GPIO39)
        .with_scl(p.GPIO38)
        .into_async();

    let i2c0_bus = Mutex::new(i2c0);
    let i2c0_bus = I2C0_BUS.init(i2c0_bus);

    let i2c_eeprom = I2cDevice::new(i2c0_bus);
    let i2c_expander = I2cDevice::new(i2c0_bus);

    // MIDI
    let uart_config = UartConfig::default().with_baudrate(31_250);

    // MIDI Thru
    let mut uart_thru = UartTx::new(p.UART1, uart_config)
        .unwrap()
        .with_tx(p.GPIO17)
        .into_async();

    // MIDI In/Out
    let mut uart_midi = Uart::new(p.UART2, uart_config)
        .unwrap()
        .with_tx(p.GPIO43)
        .with_rx(p.GPIO44)
        .into_async();

    // USB
    let usb = Usb::new(p.USB0, p.GPIO20, p.GPIO19);
    let ep_out_buffer = EP_OUT_BUFFER.init([0; 1024]);
    let usb_config = UsbConfig::default();
    let usb_driver = UsbDriver::new(usb, ep_out_buffer, usb_config);

    // AUX jacks inputs
    let aux_inputs = (p.GPIO34, p.GPIO35, p.GPIO36);

    let mut cpu_control = CpuControl::new(p.CPU_CTRL);

    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(CORE1_STACK) }, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|sp| {
                sp.spawn(main_core1(sp)).ok();
            });
        })
        .unwrap();

    let chan_x_0 = CHAN_X_0.init(Channel::new());
    let chan_max = CHAN_MAX.init(Channel::new());
    let chan_midi = CHAN_MIDI.init(Channel::new());
    let chan_leds = CHAN_LEDS.init(Channel::new());

    // TODO: Get this from eeprom
    // Fuck I think this needs to be configurable on the fly??
    let global_config = GLOBAL_CONFIG.init(config::GlobalConfig {
        clock_src: config::ClockSrc::Atom,
    });

    // spawner.spawn(read_clock(ports.port17)).unwrap();

    tasks::clock::start_clock(&spawner, chan_x_0.sender(), aux_inputs, global_config).await;

    tasks::leds::start_leds(&spawner, spi_leds, chan_leds.receiver()).await;

    tasks::max::start_max(
        &spawner,
        spi_max,
        mux_pins,
        p.GPIO10,
        chan_x_0.sender(),
        chan_max.receiver(),
    )
    .await;

    tasks::transport::start_transports(
        &spawner,
        usb_driver,
        uart_midi,
        uart_thru,
        chan_midi.receiver(),
    )
    .await;

    tasks::buttons::start_buttons(&spawner, buttons, chan_x_0.sender()).await;

    let mut eeprom = At24Cx::new(i2c_eeprom, Address(0, 0), 17, Delay);

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
                XRxMsg::MidiMessage(midi_msg) => chan_midi.send((chan, midi_msg)).await,
                XRxMsg::SetLed(action) => chan_leds.send((chan, action)).await,
            }

            // FIXME: Next steps: create a channel for each component (LEDs, MAX, MIDI) and then
            // listen on CHAN_X_RX on core 0, then send a message to the appropriate channels
        }
    };

    Timer::after_millis(100).await;

    scene_sender.send(&[1; 16]);

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
