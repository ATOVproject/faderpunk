use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver, mutex::Mutex};
use embassy_time::Timer;
use esp_hal::{
    gpio::{GpioPin, Level, Output, OutputConfig},
    spi::master::Spi,
    Async,
};
use max11300::{
    config::{
        ConfigMode0, ConfigMode5, ConfigMode7, DeviceConfig, Port, ADCCTL, ADCRANGE, AVR, DACREF,
        NSAMPLES, THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use portable_atomic::{AtomicU16, Ordering};
use static_cell::StaticCell;

use crate::{XTxMsg, XTxSender};

type SharedMax = Mutex<NoopRawMutex, Max11300<Spi<'static, Async>, Output<'static>>>;

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_DAC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_ADC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];

type MuxPins = (GpioPin<30>, GpioPin<31>, GpioPin<32>, GpioPin<33>);
type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, MaxConfig), 64>;

#[derive(Clone, Copy)]
pub enum MaxConfig {
    Mode0,
    Mode5(ConfigMode5),
    Mode7(ConfigMode7),
}

pub async fn start_max(
    spawner: &Spawner,
    spi: Spi<'static, Async>,
    mux_pins: MuxPins,
    cs: GpioPin<10>,
    x_tx: XTxSender,
    x_rx: XRxReceiver,
) {
    let device_config = DeviceConfig {
        thshdn: THSHDN::Enabled,
        dacref: DACREF::InternalRef,
        adcctl: ADCCTL::ContinuousSweep,
        ..Default::default()
    };

    let max_driver = Max11300::try_new(
        spi,
        Output::new(cs, Level::High, OutputConfig::default()),
        device_config,
    )
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
        .spawn(read_fader(mux_pins, ports.port16, x_tx))
        .unwrap();

    spawner.spawn(process_channel_values(max)).unwrap();

    spawner.spawn(reconfigure_ports(max, x_rx)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    mux_pins: MuxPins,
    max_port: Mode0Port<Spi<'static, Async>, Output<'static>, NoopRawMutex>,
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

    let mut pin0 = Output::new(mux_pins.0, Level::Low, OutputConfig::default());
    let mut pin1 = Output::new(mux_pins.1, Level::Low, OutputConfig::default());
    let mut pin2 = Output::new(mux_pins.2, Level::Low, OutputConfig::default());
    let mut pin3 = Output::new(mux_pins.3, Level::Low, OutputConfig::default());

    let mut chan: usize = 0;
    let mut prev_values: [u16; 16] = [0; 16];

    loop {
        // Set pins according to CD74HC4067 multiplexer selection logic (S0-S3)
        // S0 = pin0 (LSB), S1 = pin1, S2 = pin2, S3 = pin3 (MSB)
        pin0.set_level(if (chan & 0b0001) != 0 {
            Level::High
        } else {
            Level::Low
        }); // S0
        pin1.set_level(if (chan & 0b0010) != 0 {
            Level::High
        } else {
            Level::Low
        }); // S1
        pin2.set_level(if (chan & 0b0100) != 0 {
            Level::High
        } else {
            Level::Low
        }); // S2
        pin3.set_level(if (chan & 0b1000) != 0 {
            Level::High
        } else {
            Level::Low
        }); // S3

        // Allow time for multiplexer to settle
        Timer::after_micros(3000).await;

        let val = fader_port.get_value().await.unwrap();

        // Use inverted channel for faders as they are inverted on on the mux output
        let fader_chan = 15 - chan;

        let diff = (val as i16 - prev_values[fader_chan] as i16).unsigned_abs();
        prev_values[fader_chan] = val;

        if diff >= 4 {
            x_tx.send((fader_chan, XTxMsg::FaderChange)).await;
        }

        MAX_VALUES_FADER[fader_chan].store(val, Ordering::Relaxed);

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
