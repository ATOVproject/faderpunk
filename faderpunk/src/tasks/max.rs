use core::sync::atomic::AtomicU8;

use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio::{Config as PioConfig, Direction as PioDirection, Pio},
    spi::{self, Async, Spi},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::Timer;
use libfp::{
    latch::{AnalogLatch, LatchLayer},
    types::MaxCalibration,
    CALIBRATION_SCALE_FACTOR,
};
use max11300::{
    config::{
        ConfigMode0, ConfigMode7, DeviceConfig, Mode, Port, ADCCTL, ADCRANGE, AVR, DACREF,
        NSAMPLES, THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use portable_atomic::{AtomicBool, AtomicU16, Ordering};
use static_cell::StaticCell;

use crate::{
    events::{InputEvent, EVENT_PUBSUB},
    tasks::{
        buttons::is_scene_button_pressed,
        global_config::{
            get_fader_value_from_config, get_global_config, set_global_config_via_chan,
        },
    },
    Irqs,
};

const MAX_CHANNEL_SIZE: usize = 16;

type SharedMax = Mutex<NoopRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'static>>>;
type MuxPins = (
    Peri<'static, PIN_12>,
    Peri<'static, PIN_13>,
    Peri<'static, PIN_14>,
    Peri<'static, PIN_15>,
);

pub type MaxSender = Sender<'static, CriticalSectionRawMutex, (usize, MaxCmd), MAX_CHANNEL_SIZE>;
pub static MAX_CHANNEL: Channel<CriticalSectionRawMutex, (usize, MaxCmd), MAX_CHANNEL_SIZE> =
    Channel::new();

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_TRIGGERS_GPO: [AtomicU8; 20] = [const { AtomicU8::new(0) }; 20];
pub static MAX_VALUES_DAC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static MAX_VALUES_ADC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static CALIBRATING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum MaxCmd {
    // Mode, GPO level (for mode 3)
    ConfigurePort(Mode, Option<u16>),
    GpoSetHigh,
    GpoSetLow,
}

pub async fn start_max(
    spawner: &Spawner,
    spi0: Spi<'static, SPI0, spi::Async>,
    pio0: Peri<'static, PIO0>,
    mux_pins: MuxPins,
    cs: Peri<'static, PIN_17>,
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

    // TODO: Make individual port
    spawner
        .spawn(read_fader(pio0, mux_pins, ports.port16))
        .unwrap();

    spawner
        .spawn(process_channel_values(max, calibration_data))
        .unwrap();

    spawner.spawn(message_loop(max)).unwrap();
}

// TODO: I think we can combine all these tasks into one
#[embassy_executor::task]
async fn read_fader(
    pio0: Peri<'static, PIO0>,
    mux_pins: MuxPins,
    max_port: Mode0Port<Spi<'static, SPI0, spi::Async>, Output<'static>, NoopRawMutex>,
) {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let fader_port = max_port
        .into_configured_port(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_2v5,
            NSAMPLES::Samples1,
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

    let global_config = get_global_config();

    // Initialize first layer values (main function)
    let mut main_fader_values: [u16; 16] = [0; 16];
    for chan in 0..16 {
        let channel = 15 - chan;
        sm0.tx().wait_push(chan as u32).await;
        Timer::after_millis(1).await;
        let value = fader_port.get_value().await.unwrap();
        main_fader_values[channel] = value;
        // Also initialize the shared state
        MAX_VALUES_FADER[channel].store(value, Ordering::Relaxed);
    }

    // Initialize second layer values (global settings)
    let mut global_settings_fader_values: [u16; 16] =
        core::array::from_fn(|channel| get_fader_value_from_config(channel, &global_config));

    let mut fader_latches: [AnalogLatch; 16] =
        core::array::from_fn(|channel| AnalogLatch::new(main_fader_values[channel]));

    let mut chan: usize = 0;

    loop {
        // global config mode: Alt, normal mode: Main
        let active_layer = if is_scene_button_pressed() {
            LatchLayer::Alt
        } else {
            LatchLayer::Main
        };

        // Channels are in reverse
        let channel = 15 - chan;
        // send the channel value to the PIO state machine to trigger the program
        sm0.tx().wait_push(chan as u32).await;

        // this translates to ~60Hz refresh rate for the faders (1000 / (1 * 16) = 62.5)
        Timer::after_millis(1).await;

        let val = fader_port.get_value().await.unwrap();

        // Scale a bit across the dead-zone (~4087 -> 4095) using integer math
        let val = (((val as u32 * 1002) / 1000) as u16).clamp(0, 4095);

        let latch = &mut fader_latches[channel];

        let target_value = match active_layer {
            LatchLayer::Main => main_fader_values[channel],
            LatchLayer::Alt => global_settings_fader_values[channel],
            _ => unreachable!(),
        };

        if let Some(new_value) = latch.update(val, active_layer, target_value) {
            let diff = (new_value as i32 - target_value as i32).abs();
            match active_layer {
                LatchLayer::Main => {
                    if diff >= 4 {
                        event_publisher
                            .publish(InputEvent::FaderChange(channel))
                            .await;
                        main_fader_values[channel] = new_value;
                    }
                    MAX_VALUES_FADER[channel].store(new_value, Ordering::Relaxed)
                }
                LatchLayer::Alt => {
                    if diff >= 4 {
                        set_global_config_via_chan(channel, new_value);
                        global_settings_fader_values[channel] = new_value;
                    }
                }
                _ => unreachable!(),
            }
        }

        chan = (chan + 1) % 16;
    }
}

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
                Mode::Mode3(_) => match MAX_TRIGGERS_GPO[i].load(Ordering::Relaxed) {
                    1 => {
                        max.gpo_set_low(port).await.unwrap();
                        MAX_TRIGGERS_GPO[i].store(0, Ordering::Relaxed);
                    }
                    2 => {
                        max.gpo_set_high(port).await.unwrap();
                        MAX_TRIGGERS_GPO[i].store(0, Ordering::Relaxed);
                    }
                    _ => {}
                },
                Mode::Mode5(config) => {
                    let target_dac_value = MAX_VALUES_DAC[i].load(Ordering::Relaxed);
                    let calibrated_value = if target_dac_value == 0 {
                        // If the target is 0, the output MUST be 0
                        0
                    } else if CALIBRATING.load(Ordering::Relaxed) {
                        target_dac_value
                    } else if let Some(data) = calibration_data {
                        // Determine which DAC range is configured and use appropriate calibration data
                        let range_idx = match config.0 {
                            max11300::config::DACRANGE::Rg0_10v => 0,
                            max11300::config::DACRANGE::RgNeg5_5v => 1,
                            _ => 0, // Default to 0-10V range for other ranges
                        };
                        let (slope, intercept) = data.outputs[i][range_idx];

                        (((target_dac_value as i64 * slope)
                            + intercept
                            + (CALIBRATION_SCALE_FACTOR / 2))
                            >> 16)
                            .clamp(0, 4095) as u16
                    } else {
                        target_dac_value
                    };

                    max.dac_set_value(port, calibrated_value).await.unwrap();
                }
                Mode::Mode7(config) => {
                    let value = max.adc_get_value(port).await.unwrap();
                    let calibrated_value = if CALIBRATING.load(Ordering::Relaxed) {
                        value
                    } else if let Some(data) = calibration_data {
                        let range_idx = match config.1 {
                            max11300::config::ADCRANGE::Rg0_10v => 0,
                            max11300::config::ADCRANGE::RgNeg5_5v => 1,
                            _ => 0, // Default to 0-10V range for other ranges
                        };
                        let (slope, intercept) = data.inputs[range_idx];

                        // ideal = slope * raw + intercept
                        // Using scaled integers: ideal = (raw * slope*S + intercept*S) / S
                        // We use a bit shift for division by SCALE_FACTOR (a power of 2)
                        // We add SCALE_FACTOR/2 for rounding
                        (((value as i64 * slope) + intercept + (CALIBRATION_SCALE_FACTOR / 2))
                            >> 16)
                            .clamp(0, 4095) as u16
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
    loop {
        let (chan, msg) = MAX_CHANNEL.receive().await;
        if chan == 16 {
            // Do not process channel 16 (faders)
            continue;
        }
        let port = Port::try_from(chan).unwrap();
        let mut max = max_driver.lock().await;

        match msg {
            MaxCmd::ConfigurePort(config, gpo_level) => match config {
                Mode::Mode0(_) => {
                    max.configure_port(port, ConfigMode0).await.unwrap();
                }
                Mode::Mode3(config) => {
                    max.configure_port(port, config).await.unwrap();
                    max.gpo_configure_level(port, gpo_level.unwrap_or(2048))
                        .await
                        .unwrap();
                    max.gpo_set_high(port).await.unwrap();
                }
                Mode::Mode5(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
                Mode::Mode7(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
                _ => {}
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
