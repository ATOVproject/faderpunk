#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod app;
mod apps;
mod tasks;

use apps::run_app_by_id;
use defmt::info;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::{Executor, Spawner};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{UART0, UART1, USB};
use embassy_rp::uart;
use embassy_rp::usb;
use embassy_rp::{
    bind_interrupts,
    i2c::{self, Async, I2c},
    peripherals::{I2C1, PIO0},
    pio,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::Delay;
use is31fl3218::Is31Fl3218;

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
    UART0_IRQ => uart::InterruptHandler<UART0>;
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, I2C1, Async>>> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static mut CORE1_STACK: Stack<4096> = Stack::new();

// FIXME: create config builder to create full 16 channel layout with various apps
// The app at some point needs access to the MAX to configure it. Maybe this can happen via
// CHANNEL?
// Builder config needs to be serializable to store in eeprom

// App slots
#[embassy_executor::task(pool_size = 16)]
async fn run_app(number: usize, start_channel: usize) {
    run_app_by_id(number, start_channel).await;
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

    // FIXME: how do we re-spawn things??
    // FIXME: for now let's start with an array of app ids and map it to the spawner, also don't
    // forget to check if the channels fit
    // FIXME: Create an abstraction that maps the apps to the 16 channels of the device, for the
    // spawner to spawn
    // We need something that makes sure that nothing is used twice
    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                // FIXME: Use AtomicU16 to cancel tasks (break out when bit for channel is high)
                // We only replace ALL 16 channels at once
                for i in 0..1 {
                    spawner.spawn(run_app(1, i)).unwrap();
                }
            });
        },
    );

    tasks::max::start_max(
        &spawner, p.SPI0, p.PIO0, p.PIN_12, p.PIN_13, p.PIN_14, p.PIN_15, p.PIN_17, p.PIN_18,
        p.PIN_19, p.PIN_16, p.DMA_CH0, p.DMA_CH1,
    )
    .await;

    tasks::usb::start_usb(&spawner, p.USB).await;

    tasks::serial::start_uart(
        &spawner, p.UART0, p.UART1, p.PIN_0, p.PIN_8, p.PIN_9, p.DMA_CH2, p.DMA_CH3, p.DMA_CH4,
    )
    .await;

    // spawner.spawn(read_clock(ports.port17)).unwrap();

    let sda = p.PIN_26;
    let scl = p.PIN_27;

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
}
