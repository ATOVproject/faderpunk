#![no_std]
#![no_main]

mod apps;

use defmt::info;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::{Executor, Spawner};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::USB;
use embassy_rp::usb;
use embassy_rp::{
    bind_interrupts,
    gpio::{Level, Output},
    i2c::{self, Async, I2c},
    peripherals::{I2C1, PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio,
    spi::{self, Spi},
};
use embassy_sync::channel::Sender;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Channel,
    mutex::Mutex,
};
use embassy_time::{Delay, Timer};
use is31fl3218::Is31Fl3218;
use max11300::config::Port;
use max11300::ConfigurePort;
use max11300::Ports;
use max11300::{
    config::{
        ConfigMode5, ConfigMode7, DeviceConfig, ADCCTL, ADCRANGE, AVR, DACRANGE, DACREF, NSAMPLES,
        THSHDN,
    },
    IntoConfiguredPort, Max11300, Mode0Port,
};
use pio_proc::pio_asm;

use {defmt_rtt as _, panic_probe as _};

// FIXME: Can we use embassy LazyLock here (embassy-sync 0.7 prob)
use static_cell::StaticCell;

use at24cx::{Address, At24Cx};
use sequential_storage::{
    cache::NoCache,
    map::{fetch_item, store_item},
};

bind_interrupts!(struct Irqs {
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

static MAX: StaticCell<
    Mutex<ThreadModeRawMutex, Max11300<Spi<'static, SPI0, spi::Async>, Output>>,
> = StaticCell::new();
static FADER_VALUES: Mutex<CriticalSectionRawMutex, [u16; 16]> = Mutex::new([0u16; 16]);
// FIXME: Maybe we can use another Mutex for the DAC (!) values that we want to set, this way
// we can also send out the values at a constant rate and only take the ones into account that
// are currently present in the mutex array
static DAC_VALUES: Mutex<CriticalSectionRawMutex, [Option<u16>; 16]> = Mutex::new([None; 16]);
static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, I2C1, Async>>> = StaticCell::new();
static CHANNEL: Channel<CriticalSectionRawMutex, Action, 16> = Channel::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static mut CORE1_STACK: Stack<4096> = Stack::new();

enum Action {}

// FIXME: Rename
struct App<const N: usize> {
    channels: [usize; N],
}

impl<const N: usize> App<N> {
    fn new(channels: [usize; N]) -> Self {
        Self { channels }
    }

    async fn get_fader_values(&self) -> [u16; N] {
        let fader_values = FADER_VALUES.lock().await;
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = fader_values[self.channels[i]];
        }
        buf
    }

    // FIXME: Ultimately this API should also reflect the type of port (like in the MAX driver)
    async fn set_dac_values(&self, values: [u16; N]) {
        let mut dac_values = DAC_VALUES.lock().await;
        for i in 0..N {
            dac_values[self.channels[i]] = Some(values[i]);
        }
    }
}

// FIXME: create config builder to create full 16 channel layout with various apps
// We need something that makes sure that nothing is used twice

// App slots
#[embassy_executor::task(pool_size = 16)]
async fn run_app(channel: usize) {
    // FIXME: Here we need to get the exact channnels the app is using. This is probably coming
    // from the builder above
    // For now it's hardcoded to one channel
    let args = App::new([channel]);
    apps::default::run(args).await;
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    pin12: PIN_12,
    pin13: PIN_13,
    pin14: PIN_14,
    pin15: PIN_15,
    max_port: Mode0Port<Spi<'_, SPI0, spi::Async>, Output<'_>, ThreadModeRawMutex>,
) {
    let fader_port = max_port
        .into_configured_port(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_2v5,
            NSAMPLES::Samples16,
        ))
        .await
        .unwrap();

    let pio::Pio {
        mut common,
        mut sm0,
        ..
    } = pio::Pio::new(pio0, Irqs);

    let prg = pio_asm!(
        "
        pull block
        out pins, 4
        "
    );
    let pin0 = common.make_pio_pin(pin12);
    let pin1 = common.make_pio_pin(pin13);
    let pin2 = common.make_pio_pin(pin14);
    let pin3 = common.make_pio_pin(pin15);
    sm0.set_pin_dirs(pio::Direction::Out, &[&pin0, &pin1, &pin2, &pin3]);
    let mut cfg = pio::Config::default();
    cfg.set_out_pins(&[&pin0, &pin1, &pin2, &pin3]);
    cfg.use_program(&common.load_program(&prg.program), &[]);
    sm0.set_config(&cfg);
    sm0.set_enable(true);

    let mut chan: usize = 0;

    loop {
        // send the channel value to the PIO state machine to trigger the program
        sm0.tx().wait_push(chan as u32).await;

        // this translates to ~30Hz refresh rate for the faders
        Timer::after_millis(2).await;

        let val = fader_port.get_value().await.unwrap();

        let mut fader_values = FADER_VALUES.lock().await;
        // pins are reversed
        fader_values[15 - chan] = val;
        chan = (chan + 1) % 16;
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

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                // FIXME: Use AtomicU16 to cancel tasks (break out when bit for channel is high)
                for i in 0..16 {
                    spawner.spawn(run_app(i)).unwrap();
                }
            });
        },
    );

    let sda = p.PIN_26;
    let scl = p.PIN_27;
    let cs = p.PIN_17;
    let clk = p.PIN_18;
    let mosi = p.PIN_19;
    let miso = p.PIN_16;
    let mut spi_config = spi::Config::default();
    spi_config.frequency = 20_000_000;
    let spi = Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, spi_config);

    let device_config = DeviceConfig {
        thshdn: THSHDN::Enabled,
        dacref: DACREF::InternalRef,
        adcctl: ADCCTL::ContinuousSweep,
        ..Default::default()
    };

    let max_driver = Max11300::try_new(spi, Output::new(cs, Level::High), device_config)
        .await
        .unwrap();

    let max = MAX.init(Mutex::new(max_driver));

    // FIXME: Create an abstraction to be able to create just one port
    let ports = Ports::new(max);

    spawner
        .spawn(read_fader(
            p.PIO0,
            p.PIN_12,
            p.PIN_13,
            p.PIN_14,
            p.PIN_15,
            ports.port16,
        ))
        .unwrap();
    // spawner.spawn(read_clock(ports.port17)).unwrap();

    let i2c = i2c::I2c::new_async(p.I2C1, scl, sda, Irqs, i2c::Config::default());

    let i2c_bus = Mutex::new(i2c);
    let i2c_bus = I2C_BUS.init(i2c_bus);

    let i2c_dev0 = I2cDevice::new(i2c_bus);
    let i2c_dev1 = I2cDevice::new(i2c_bus);

    let mut led_driver = Is31Fl3218::new(i2c_dev0);

    led_driver.enable_device().await.unwrap();
    led_driver.enable_all().await.unwrap();
    led_driver.set_all(&[255; 18]).await.unwrap();

    let mut eeprom = At24Cx::new(i2c_dev1, Address(0, 0), 17, Delay);

    // These are the flash addresses in which the crate will operate.
    // The crate will not read, write or erase outside of this range.
    let flash_range = 0x1000..0x3000;
    // We need to give the crate a buffer to work with.
    // It must be big enough to serialize the biggest value of your storage type in,
    // rounded up to to word alignment of the flash. Some kinds of internal flash may require
    // this buffer to be aligned in RAM as well.
    let mut data_buffer = [0; 128];

    // Now we store an item the flash with key 42.
    // Again we make sure we pass the correct key and value types, u8 and u32.
    // It is important to do this consistently.

    store_item(
        &mut eeprom,
        flash_range.clone(),
        &mut NoCache::new(),
        &mut data_buffer,
        &42u8,
        &104729u32,
    )
    .await
    .unwrap();

    // When we ask for key 42, we not get back a Some with the correct value

    assert_eq!(
        fetch_item::<u8, u32, _>(
            &mut eeprom,
            flash_range.clone(),
            &mut NoCache::new(),
            &mut data_buffer,
            &42,
        )
        .await
        .unwrap(),
        Some(104729)
    );

    // FIXME: This needs to happen inside the apps themselves using a proper abstraction
    // Also this is obviously only one way of configuring
    let mut max_driver = max.lock().await;
    for i in 0..16 {
        let port = Port::try_from(i).unwrap();
        max_driver
            .configure_port(port, ConfigMode5(DACRANGE::Rg0_10v))
            .await
            .unwrap();
    }
    drop(max_driver);

    // FIXME: Maybe we can run this on a higher priority (and also spawn it as a task)
    loop {
        let mut max_driver = max.lock().await;
        let dac_values = DAC_VALUES.lock().await;
        for (i, &value) in dac_values.iter().enumerate() {
            let port = Port::try_from(i).unwrap();
            if let Some(val) = value {
                max_driver.dac_set_value(port, val).await.unwrap();
            }
        }
        Timer::after_micros(500).await;
    }
}
