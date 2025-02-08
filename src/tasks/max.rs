use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio,
    spi::{self, Async, Spi},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::Receiver,
    mutex::Mutex,
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
use portable_atomic::{AtomicU16, Ordering};
use static_cell::StaticCell;

use crate::{Irqs, XTxMsg, XTxSender};

type SharedMax =
    Mutex<CriticalSectionRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'static>>>;

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_DAC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_ADC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];

type MuxPins = (PIN_12, PIN_13, PIN_14, PIN_15);
type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, MaxConfig), 64>;

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
    mux_pins: MuxPins,
    cs: PIN_17,
    x_tx: XTxSender,
    x_rx: XRxReceiver,
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

    // TODO: Create an abstraction to be able to create just one port
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

    // TODO: Make individual port
    spawner
        .spawn(read_fader(pio0, mux_pins, ports.port16, x_tx))
        .unwrap();

    spawner.spawn(process_channel_values(max)).unwrap();

    spawner.spawn(reconfigure_ports(max, x_rx)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    mux_pins: MuxPins,
    max_port: Mode0Port<Spi<'static, SPI0, spi::Async>, Output<'static>, CriticalSectionRawMutex>,
    x_tx: XTxSender,
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
    let pin0 = common.make_pio_pin(mux_pins.0);
    let pin1 = common.make_pio_pin(mux_pins.1);
    let pin2 = common.make_pio_pin(mux_pins.2);
    let pin3 = common.make_pio_pin(mux_pins.3);
    sm0.set_pin_dirs(pio::Direction::Out, &[&pin0, &pin1, &pin2, &pin3]);
    let mut cfg = pio::Config::default();
    cfg.set_out_pins(&[&pin0, &pin1, &pin2, &pin3]);
    cfg.use_program(&common.load_program(&prg.program), &[]);
    sm0.set_config(&cfg);
    sm0.set_enable(true);

    let mut chan: usize = 0;
    let mut prev_values: [u16; 16] = [0; 16];

    loop {
        // Channels are in reverse
        let channel = 15 - chan;
        // send the channel value to the PIO state machine to trigger the program
        sm0.tx().wait_push(chan as u32).await;

        // this translates to ~30Hz refresh rate for the faders (1000 / (2 * 16) = 31.25)
        Timer::after_millis(2).await;

        let val = fader_port.get_value().await.unwrap();
        let diff = (val as i16 - prev_values[channel] as i16).unsigned_abs();
        // resolution of 256 should cover the full MIDI 1.0 range
        prev_values[channel] = val;

        if diff >= 4 {
            x_tx.send((channel, XTxMsg::FaderChange)).await;
            // MAX_CHANGED_FADER[15 - chan].store(true, Ordering::Relaxed);
        }

        MAX_VALUES_FADER[channel].store(val, Ordering::Relaxed);

        chan = (chan + 1) % 16;
    }
}

#[embassy_executor::task]
async fn process_channel_values(max_driver: &'static SharedMax) {
    loop {
        // hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;
        let mut max = max_driver.lock().await;

        for i in 0..16 {
            let port = Port::try_from(i).unwrap();
            match max.get_mode(port) {
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
async fn reconfigure_ports(max_driver: &'static SharedMax, x_rx: XRxReceiver) {
    // TODO: Put MAX port in hi-impedance mode when using the internal GPIO interrupts
    loop {
        let (chan, config) = x_rx.receive().await;
        let port = Port::try_from(chan).unwrap();
        let mut max = max_driver.lock().await;

        match config {
            MaxConfig::Mode0 => {
                max.configure_port(port, ConfigMode0).await.unwrap();
            }
            MaxConfig::Mode5(config) => {
                max.configure_port(port, config).await.unwrap();
            }
            MaxConfig::Mode7(config) => {
                max.configure_port(port, config).await.unwrap();
            }
        }
    }
}
