#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::config::Config;
use embassy_rp::{bind_interrupts, i2c, peripherals::I2C1};
use embassy_time::Timer;

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let config = Config::default();

    let p = embassy_rp::init(config);

    // SPI0
    // let mut spi0_config = spi::Config::default();
    // spi0_config.frequency = 20_000_000;
    // let spi0 = Spi::new(
    //     p.SPI0,
    //     p.PIN_18,
    //     p.PIN_19,
    //     p.PIN_16,
    //     p.DMA_CH0,
    //     p.DMA_CH1,
    //     spi0_config,
    // );

    // I2C1
    // let mut i2c1_config = i2c::Config::default();
    // i2c1_config.frequency = 1_000_000;
    // let i2c1 = i2c::I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2c1_config);

    loop {
        Timer::after_millis(1000).await;
        defmt::info!("PING");
    }
}
