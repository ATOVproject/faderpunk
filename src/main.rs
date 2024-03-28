#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts,
    gpio::{Level, Output},
    i2c::{self, Config as I2cConfig, InterruptHandler as I2cInterruptHandler},
    peripherals::{I2C1, PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio::{Config, Direction, InterruptHandler as PioInterruptHandler, Pio},
    spi::{Async, Config as SpiConfig, Spi},
};
use embassy_time::{Duration, Timer};
use is31fl3218::Is31Fl3218;
use max11300::{
    config::{
        ConfigMode5, ConfigMode7, DeviceConfig, ADCCTL, ADCRANGE, AVR, DACRANGE, DACREF,
        NSAMPLES, THSHDN,
    },
    ConfiguredMax11300, IntoConfiguredPort, Max11300, Mode0Port,
};
use pio_proc::pio_asm;

use {defmt_rtt as _, panic_probe as _};

use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    I2C1_IRQ => I2cInterruptHandler<I2C1>;
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
});

static MAX: StaticCell<ConfiguredMax11300<Spi<'static, SPI0, Async>, Output<'static, PIN_17>>> =
    StaticCell::new();

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    pin12: PIN_12,
    pin13: PIN_13,
    pin14: PIN_14,
    pin15: PIN_15,
    max_port: Mode0Port<'static, Spi<'static, SPI0, Async>, Output<'static, PIN_17>>,
) {
    let fader_port = max_port
        .into_configured_port(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_2v5,
            NSAMPLES::Samples16,
        ))
        .await
        .unwrap();

    let Pio {
        mut common,
        mut sm0,
        ..
    } = Pio::new(pio0, Irqs);

    let prg = pio_asm!(
        "
            start:
                set x, 15
            loop:
                pull block
                mov osr, !x
                out pins, 4
                jmp !x start
                jmp x-- loop
        "
    );
    let pin0 = common.make_pio_pin(pin12);
    let pin1 = common.make_pio_pin(pin13);
    let pin2 = common.make_pio_pin(pin14);
    let pin3 = common.make_pio_pin(pin15);
    sm0.set_pin_dirs(Direction::Out, &[&pin0, &pin1, &pin2, &pin3]);
    let mut cfg = Config::default();
    cfg.set_out_pins(&[&pin0, &pin1, &pin2, &pin3]);
    cfg.use_program(&common.load_program(&prg.program), &[]);
    sm0.set_config(&cfg);
    sm0.set_enable(true);

    loop {
        // Send any value to the PIO state machine to trigger the program
        sm0.tx().wait_push(0).await;

        // FIXME: we probably need to wait some time for the ADC to settle here
        let val = fader_port.get_value().await.unwrap();

        info!("VAL: {:?}", val);

        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_26;
    let scl = p.PIN_27;
    let cs = p.PIN_17;
    let clk = p.PIN_18;
    let mosi = p.PIN_19;
    let miso = p.PIN_16;
    let spi_config = SpiConfig::default();
    let spi = Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, spi_config);

    let max = Max11300::new(spi, Output::new(cs, Level::High));

    let device_config = DeviceConfig {
        thshdn: THSHDN::Enabled,
        dacref: DACREF::InternalRef,
        adcctl: ADCCTL::ContinuousSweep,
        ..Default::default()
    };
    let max = max.into_configured(device_config).await.unwrap();
    let max_foo = MAX.init(max);

    let ports = max_foo.split();

    // channel ports

    let port0 = ports.port0.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port1 = ports.port1.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port2 = ports.port2.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port3 = ports.port3.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port4 = ports.port4.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port5 = ports.port5.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port6 = ports.port6.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port7 = ports.port7.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port8 = ports.port8.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port9 = ports.port9.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port10 = ports.port10.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port11 = ports.port11.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port12 = ports.port12.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port13 = ports.port13.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port14 = ports.port14.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port15 = ports.port15.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();

    port0.set_value(65_535).await.unwrap();
    port1.set_value(65_535).await.unwrap();
    port2.set_value(65_535).await.unwrap();
    port3.set_value(65_535).await.unwrap();
    port4.set_value(65_535).await.unwrap();
    port5.set_value(65_535).await.unwrap();
    port6.set_value(65_535).await.unwrap();
    port7.set_value(65_535).await.unwrap();
    port8.set_value(65_535).await.unwrap();
    port9.set_value(65_535).await.unwrap();
    port10.set_value(65_535).await.unwrap();
    port11.set_value(65_535).await.unwrap();
    port12.set_value(65_535).await.unwrap();
    port13.set_value(65_535).await.unwrap();
    port14.set_value(65_535).await.unwrap();
    port15.set_value(65_535).await.unwrap();

    // AUX ports

    let port17 = ports.port17.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port18 = ports.port18.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();
    let port19 = ports.port19.into_configured_port(ConfigMode5(DACRANGE::Rg0_10v)).await.unwrap();

    port17.set_value(65_535).await.unwrap();
    port18.set_value(65_535).await.unwrap();
    port19.set_value(65_535).await.unwrap();

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

    let i2c = i2c::I2c::new_async(p.I2C1, scl, sda, Irqs, I2cConfig::default());
    let mut led_driver = Is31Fl3218::new(i2c);

    led_driver.enable_device().await.unwrap();
    led_driver.enable_all().await.unwrap();
    led_driver.set_all(&[255; 18]).await.unwrap();

    // let i2c = i2c::I2c::new_blocking(p.I2C1, scl, sda, Config::default());
    // let mut pca9555 = Pca9555::new(i2c, false, false, false);
    // let pca_pins = pca9555.split();
    // let io1_0 = pca_pins.io1_0;
    //
    // let mut int = Input::new(p.PIN_1, Pull::Up);
    //
    // // Timer::after_secs(1).await;
    //
    // let is_high = int.is_high();
    // //
    // info!("INT IS HIGH: {:?}", is_high);

    // let mut int = Input::new(p.PIN_3, Pull::None);
    // let mut val;

    loop {
        // val = port16.get_value().await.unwrap();
        // info!("INT IS HIGH: {:?}", int.is_high());
        // info!("VAL: {:?}", val);
        // int.wait_for_any_edge().await;
        // info!("EDGE DETECTED");
        // port0.set_value(65535).await.unwrap();
        //
        // // let is_low = io1_0.is_low().unwrap();
        // // let int_is_low = int.is_low();
        // // if int_is_low {
        // //     info!("INT IS LOW: {:?}", int_is_low);
        // // }
        //
        // // info!("BTN IS LOW: {:?}", is_low);
        //
        // Timer::after_secs(1).await;
        //
        // port0.set_value(0).await.unwrap();
        //
        Timer::after_secs(1).await;
    }
}
