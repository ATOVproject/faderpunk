use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{
        DMA_CH0, DMA_CH1, PIN_12, PIN_13, PIN_14, PIN_15, PIN_16, PIN_17, PIN_18, PIN_19, PIO0,
        SPI0,
    },
    pio,
    spi::{self, Async, Spi},
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex,
    pubsub::PubSubChannel,
};
use embassy_time::Timer;
use max11300::{
    config::{
        ConfigMode5, ConfigMode7, DeviceConfig, Port, ADCCTL, ADCRANGE, AVR, DACREF, NSAMPLES,
        THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use pio_proc::pio_asm;
use portable_atomic::{AtomicU16, Ordering};
use static_cell::StaticCell;

use crate::Irqs;

static MAX: StaticCell<
    Mutex<CriticalSectionRawMutex, Max11300<Spi<'static, SPI0, spi::Async>, Output>>,
> = StaticCell::new();
pub static MAX_VALUES_ADC: Mutex<CriticalSectionRawMutex, [u16; 16]> = Mutex::new([0; 16]);
pub static MAX_VALUES_DAC: Mutex<CriticalSectionRawMutex, [Option<u16>; 16]> =
    Mutex::new([None; 16]);
pub static MAX_VALUES_FADERS: Mutex<CriticalSectionRawMutex, [u16; 16]> = Mutex::new([0u16; 16]);
pub static MAX_MASK_RECONFIGURE: AtomicU16 = AtomicU16::new(0);
pub static MAX_CHANNEL_RECONFIGURE: Channel<CriticalSectionRawMutex, MaxReconfigureAction, 16> =
    Channel::new();
pub static MAX_PUBSUB_FADER_CHANGED: PubSubChannel<CriticalSectionRawMutex, usize, 4, 16, 1> =
    PubSubChannel::new();

// FIXME: Can we make all chans u8 for some memory savings???
pub enum MaxReconfigureAction {
    Mode5(usize, ConfigMode5),
    Mode7(usize, ConfigMode7),
}

pub async fn start_max(
    spawner: &Spawner,
    spi0: SPI0,
    pio0: PIO0,
    mux0: PIN_12,
    mux1: PIN_13,
    mux2: PIN_14,
    mux3: PIN_15,
    cs: PIN_17,
    clk: PIN_18,
    mosi: PIN_19,
    miso: PIN_16,
    dma0: DMA_CH0,
    dma1: DMA_CH1,
) {
    let mut spi_config = spi::Config::default();
    spi_config.frequency = 20_000_000;
    let spi = Spi::new(spi0, clk, mosi, miso, dma0, dma1, spi_config);

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

    // FIXME: Make individual port
    spawner
        .spawn(read_fader(pio0, mux0, mux1, mux2, mux3, ports.port16))
        .unwrap();

    spawner.spawn(write_dac_values(max)).unwrap();

    spawner.spawn(reconfigure_ports(max)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    pin12: PIN_12,
    pin13: PIN_13,
    pin14: PIN_14,
    pin15: PIN_15,
    max_port: Mode0Port<Spi<'_, SPI0, spi::Async>, Output<'_>, CriticalSectionRawMutex>,
) {
    let fader_port = max_port
        .into_configured_port(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_10v,
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
    let mut prev_values: [u16; 16] = [0; 16];
    let change_publisher = MAX_PUBSUB_FADER_CHANGED.publisher().unwrap();

    loop {
        // send the channel value to the PIO state machine to trigger the program
        sm0.tx().wait_push(chan as u32).await;

        // this translates to ~30Hz refresh rate for the faders (1000 / (2 * 16) = 31.25)
        Timer::after_millis(2).await;

        let val = fader_port.get_value().await.unwrap();
        let diff = (val as i16 - prev_values[15 - chan] as i16).unsigned_abs();
        // resolution of 256 should cover the full MIDI 1.0 range
        prev_values[15 - chan] = val;

        if diff >= 4 {
            // we publish immediate and don't really care so much about lost messages
            change_publisher.publish_immediate(15 - chan);
        }

        let mut fader_values = MAX_VALUES_FADERS.lock().await;
        // pins are reversed
        fader_values[15 - chan] = val;
        chan = (chan + 1) % 16;
    }
}

#[embassy_executor::task]
async fn write_dac_values(
    max: &'static Mutex<CriticalSectionRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'_>>>,
) {
    loop {
        // hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;
        let mut max_driver = max.lock().await;
        let mut dac_values = MAX_VALUES_DAC.lock().await;
        for (i, value) in dac_values.iter_mut().enumerate() {
            // FIXME: Unsure about the port thing
            let port = Port::try_from(i).unwrap();
            if let Some(val) = value {
                max_driver.dac_set_value(port, *val).await.unwrap();
                // Reset all DAC values after they were set
                *value = None;
            }
        }
    }
}

#[embassy_executor::task]
async fn reconfigure_ports(
    max: &'static Mutex<CriticalSectionRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'_>>>,
) {
    loop {
        // FIXME: This match has a lot of duplication, let's see if we can improve this somehow
        // (Can the Config be an enum after all? Maybe we just need the structs for type signalling)
        match MAX_CHANNEL_RECONFIGURE.receive().await {
            MaxReconfigureAction::Mode5(chan, config) => {
                let mut max_driver = max.lock().await;
                // FIXME: Unsure about the port thing
                let port = Port::try_from(chan).unwrap();
                max_driver.configure_port(port, config).await.unwrap();
                // Set the corresponding bit in the reconfigure mask, to signal completion
                let mask = 1 << chan;
                MAX_MASK_RECONFIGURE.fetch_or(mask, Ordering::SeqCst);
            }
            MaxReconfigureAction::Mode7(chan, config) => {
                let mut max_driver = max.lock().await;
                // FIXME: Unsure about the port thing
                let port = Port::try_from(chan).unwrap();
                max_driver.configure_port(port, config).await.unwrap();
                // Set the corresponding bit in the reconfigure mask, to signal completion
                let mask = 1 << chan;
                MAX_MASK_RECONFIGURE.fetch_or(mask, Ordering::SeqCst);
            }
        }
    }
}
