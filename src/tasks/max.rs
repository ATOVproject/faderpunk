use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_12, PIN_13, PIN_14, PIN_15, PIN_16, PIN_17, PIN_18, PIN_19, PIO0, SPI0},
    pio,
    spi::{self, Async, Spi},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Channel,
    mutex::Mutex,
    pubsub::PubSubChannel,
};
use embassy_time::Timer;
use max11300::{
    config::{
        ConfigMode0, ConfigMode5, ConfigMode7, DeviceConfig, Port, ADCCTL, ADCRANGE, AVR, DACREF,
        NSAMPLES, THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use pio_proc::pio_asm;
use portable_atomic::{AtomicU16, AtomicU8, Ordering};
use static_cell::StaticCell;

use crate::Irqs;

static MAX: StaticCell<
    Mutex<CriticalSectionRawMutex, Max11300<Spi<'static, SPI0, spi::Async>, Output>>,
> = StaticCell::new();
pub static MAX_VALUES_DAC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_ADC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
static MAX_CHANNEL_CONFIG: [AtomicU8; 16] = [const { AtomicU8::new(0) }; 16];
// FIXME: I think we can cobine this one with the one above. For no reconfiguration we can just use
// the value 255 or so
pub static MAX_MASK_RECONFIGURE: AtomicU16 = AtomicU16::new(0);
pub static MAX_CHANNEL_RECONFIGURE: Channel<CriticalSectionRawMutex, MaxReconfigureAction, 16> =
    Channel::new();
pub static MAX_PUBSUB_FADER_CHANGED: PubSubChannel<CriticalSectionRawMutex, usize, 4, 16, 1> =
    PubSubChannel::new();

pub type MaxReconfigureAction = (usize, MaxConfig);

#[derive(Clone, Copy)]
pub enum MaxConfig {
    Mode0,
    Mode5(ConfigMode5),
    Mode7(ConfigMode7),
}

pub async fn start_max(
    spawner: &Spawner,
    spi0: Spi<'static, SPI0, spi::Async>,
    pio0: PIO0,
    mux0: PIN_12,
    mux1: PIN_13,
    mux2: PIN_14,
    mux3: PIN_15,
    cs: PIN_17,
) {
    let device_config = DeviceConfig {
        thshdn: THSHDN::Enabled,
        dacref: DACREF::InternalRef,
        adcctl: ADCCTL::ContinuousSweep,
        ..Default::default()
    };

    let max_driver = Max11300::try_new(spi0, Output::new(cs, Level::High), device_config)
        .await
        .unwrap();

    let max = MAX.init(Mutex::new(max_driver));

    // FIXME: Create an abstraction to be able to create just one port
    let ports = Ports::new(max);

    // Put ports 17-19 into hi-impedance mode for interrupt testing
    ports
        .port17
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();
    ports
        .port18
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();
    ports
        .port19
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();

    // FIXME: Make individual port
    spawner
        .spawn(read_fader(pio0, mux0, mux1, mux2, mux3, ports.port16))
        .unwrap();

    spawner.spawn(process_channel_values(max)).unwrap();

    spawner.spawn(reconfigure_ports(max)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    pin12: PIN_12,
    pin13: PIN_13,
    pin14: PIN_14,
    pin15: PIN_15,
    max_port: Mode0Port<Spi<'static, SPI0, spi::Async>, Output<'static>, CriticalSectionRawMutex>,
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
    let mut prev_values: [u16; 16] = [0; 16];
    let change_publisher = MAX_PUBSUB_FADER_CHANGED.publisher().unwrap();

    // let pin0 = Output::new(pin12, Level::High);
    // let pin1 = Output::new(pin13, Level::Low);
    // let pin2 = Output::new(pin14, Level::Low);
    // let pin3 = Output::new(pin15, Level::High);

    loop {
        // let val = fader_port.get_value().await.unwrap();
        //
        // info!("FADER VAL: {}", val);
        // Timer::after_secs(2).await;

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

        MAX_VALUES_FADER[15 - chan].store(val, Ordering::Relaxed);

        chan = (chan + 1) % 16;
    }
}

#[embassy_executor::task]
async fn process_channel_values(
    max_driver: &'static Mutex<
        CriticalSectionRawMutex,
        Max11300<Spi<'static, SPI0, Async>, Output<'static>>,
    >,
) {
    loop {
        // hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;
        let mut max = max_driver.lock().await;
        for (i, atomic_config) in MAX_CHANNEL_CONFIG.iter().enumerate() {
            // FIXME: Unsure about the port thing
            let port = Port::try_from(i).unwrap();
            // Ordering::Acquire provides strong consistency guarantees on a single thread
            match atomic_config.load(Ordering::Acquire) {
                5 => {
                    let value = MAX_VALUES_DAC[i].load(Ordering::Relaxed);
                    max.dac_set_value(port, value).await.unwrap();
                }
                7 => {
                    let value = max.adc_get_value(port).await.unwrap();
                    MAX_VALUES_ADC[i].store(value, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }
}

#[embassy_executor::task]
async fn reconfigure_ports(
    max_driver: &'static Mutex<
        CriticalSectionRawMutex,
        Max11300<Spi<'static, SPI0, Async>, Output<'static>>,
    >,
) {
    // FIXME: Put MAX port in hi-impedance mode when using the internal GPIO interrupts
    loop {
        let (chan, config_mode) = MAX_CHANNEL_RECONFIGURE.receive().await;
        let mut max = max_driver.lock().await;

        let port = Port::try_from(chan).unwrap();

        // FIXME: This match has a lot of duplication, let's see if we can improve this somehow
        match config_mode {
            MaxConfig::Mode0 => {
                max.configure_port(port, ConfigMode0).await.unwrap();
                // Ordering::Release provides strong consistency guarantees on a single thread
                MAX_CHANNEL_CONFIG[chan].store(0, Ordering::Release);
            }
            MaxConfig::Mode5(config) => {
                let port = Port::try_from(chan).unwrap();
                max.configure_port(port, config).await.unwrap();
                // Ordering::Release provides strong consistency guarantees on a single thread
                MAX_CHANNEL_CONFIG[chan].store(5, Ordering::Release);
            }
            MaxConfig::Mode7(config) => {
                let port = Port::try_from(chan).unwrap();
                max.configure_port(port, config).await.unwrap();
                // Ordering::Release provides strong consistency guarantees on a single thread
                MAX_CHANNEL_CONFIG[chan].store(7, Ordering::Release);
            }
        }
        // Set the corresponding bit in the reconfigure mask, to signal completion
        let mask = 1 << chan;
        MAX_MASK_RECONFIGURE.fetch_or(mask, Ordering::SeqCst);
    }
}
