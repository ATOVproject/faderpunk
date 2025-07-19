use defmt::Format;
use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio::{Config as PioConfig, Direction as PioDirection, Pio},
    spi::{self, Async, Spi},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::Timer;
use libfp::{constants::CHAN_LED_MAP, types::RegressionValues};
use libm::roundf;
use max11300::{
    config::{
        ConfigMode0, ConfigMode3, ConfigMode5, ConfigMode7, DeviceConfig, Port, ADCCTL, ADCRANGE,
        AVR, DACREF, NSAMPLES, THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use portable_atomic::{AtomicBool, AtomicU16, Ordering};
use serde::{Deserialize, Serialize};
use static_cell::StaticCell;

use crate::{
    events::{InputEvent, EVENT_PUBSUB},
    Irqs,
};

const MAX_CHANNEL_SIZE: usize = 16;

type SharedMax = Mutex<NoopRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'static>>>;
type MuxPins = (PIN_12, PIN_13, PIN_14, PIN_15);

pub type MaxSender = Sender<'static, CriticalSectionRawMutex, (usize, MaxCmd), MAX_CHANNEL_SIZE>;
pub static MAX_CHANNEL: Channel<CriticalSectionRawMutex, (usize, MaxCmd), MAX_CHANNEL_SIZE> =
    Channel::new();

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_DAC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_ADC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static CALIBRATING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
pub enum MaxCmd {
    ConfigurePort(MaxConfig),
    GpoSetHigh,
    GpoSetLow,
}

#[derive(Clone, Copy)]
pub enum MaxConfig {
    Mode0,
    Mode3(ConfigMode3, u16),
    Mode5(ConfigMode5),
    Mode7(ConfigMode7),
}

#[derive(Clone, Copy, Serialize, Deserialize, Default, Format)]
pub struct MaxCalibration {
    // (slope, intercept)
    pub inputs: (f32, f32),
    // (slope, intercept)
    pub outputs: RegressionValues,
}

pub async fn start_max(
    spawner: &Spawner,
    spi0: Spi<'static, SPI0, spi::Async>,
    pio0: PIO0,
    mux_pins: MuxPins,
    cs: PIN_17,
    calibration_data: Option<MaxCalibration>,
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
        .spawn(read_fader(pio0, mux_pins, ports.port16))
        .unwrap();

    spawner
        .spawn(process_channel_values(max, calibration_data))
        .unwrap();

    spawner.spawn(message_loop(max)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    mux_pins: MuxPins,
    max_port: Mode0Port<Spi<'static, SPI0, spi::Async>, Output<'static>, NoopRawMutex>,
) {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

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

    let prg = pio::pio_asm!(
        "
        pull block
        out pins, 4
        "
    );
    let pin0 = common.make_pio_pin(mux_pins.0);
    let pin1 = common.make_pio_pin(mux_pins.1);
    let pin2 = common.make_pio_pin(mux_pins.2);
    let pin3 = common.make_pio_pin(mux_pins.3);
    sm0.set_pin_dirs(PioDirection::Out, &[&pin0, &pin1, &pin2, &pin3]);
    let mut cfg = PioConfig::default();
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

        // this translates to ~60Hz refresh rate for the faders (1000 / (1 * 16) = 62.5)
        Timer::after_millis(1).await;

        let val = fader_port.get_value().await.unwrap();
        let diff = (val as i16 - prev_values[channel] as i16).unsigned_abs();
        // resolution of 256 should cover the full MIDI 1.0 range
        prev_values[channel] = val;

        if diff >= 4 {
            event_publisher
                .publish(InputEvent::FaderChange(channel))
                .await;
        }

        MAX_VALUES_FADER[channel].store(val, Ordering::Relaxed);

        chan = (chan + 1) % 16;
    }
}

// TODO: Should we make this message based?
#[embassy_executor::task]
async fn process_channel_values(
    max_driver: &'static SharedMax,
    calibration_data: Option<MaxCalibration>,
) {
    loop {
        // Hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;

        // Do not process channel 16 (faders)
        for i in (0..16).chain(17..20) {
            let port = Port::try_from(i).unwrap();
            let mut max = max_driver.lock().await;
            match max.get_mode(port) {
                5 => {
                    let target_dac_value = MAX_VALUES_DAC[i].load(Ordering::Relaxed);
                    let calibrated_value = if target_dac_value == 0 {
                        // If the target is 0, the output MUST be 0
                        0
                    // TODO: Is this fast enough, can we safely do that in this tight loop?
                    } else if CALIBRATING.load(Ordering::Relaxed) {
                        target_dac_value
                    } else if let Some(data) = calibration_data {
                        // For any non-zero target, apply the pre-correction formula
                        let (slope, intercept) = data.outputs[i];
                        let target_f32 = target_dac_value as f32;
                        let raw_f32 = (target_f32 - intercept) / (1.0 + slope);

                        // Round and clamp to the valid DAC range
                        roundf(raw_f32).clamp(0.0, 4095.0) as u16
                    } else {
                        target_dac_value
                    };

                    max.dac_set_value(port, calibrated_value).await.unwrap();
                }
                7 => {
                    let value = max.adc_get_value(port).await.unwrap();
                    let calibrated_value = if CALIBRATING.load(Ordering::Relaxed) {
                        value
                    } else if let Some(data) = calibration_data {
                        let (slope, intercept) = data.inputs;
                        let raw_value_f32 = value as f32;
                        let corrected_f32 = raw_value_f32 * (1.0 + slope) + intercept;

                        roundf(corrected_f32).clamp(0.0, 4095.0) as u16
                    } else {
                        value
                    };
                    MAX_VALUES_ADC[i].store(calibrated_value, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }
}

#[embassy_executor::task]
async fn message_loop(max_driver: &'static SharedMax) {
    // TODO: Put ports 17-19 in hi-impedance mode when using the internal GPIO interrupts
    loop {
        let (chan, msg) = MAX_CHANNEL.receive().await;
        if chan == 16 {
            // Do not process channel 16 (faders)
            continue;
        }
        let port = Port::try_from(chan).unwrap();
        let mut max = max_driver.lock().await;

        match msg {
            MaxCmd::ConfigurePort(config) => match config {
                MaxConfig::Mode0 => {
                    max.configure_port(port, ConfigMode0).await.unwrap();
                }
                MaxConfig::Mode3(config, gpo_level) => {
                    max.dac_set_value(port, gpo_level).await.unwrap();
                    max.configure_port(port, config).await.unwrap();
                }
                MaxConfig::Mode5(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
                MaxConfig::Mode7(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
            },
            MaxCmd::GpoSetHigh => {
                max.gpo_set_high(port).await.unwrap();
            }
            MaxCmd::GpoSetLow => {
                max.gpo_set_low(port).await.unwrap();
            }
        }
    }
}
