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
use embassy_time::{Instant, Timer};
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

/// Diagnostic instrumentation for fader ADC noise. When enabled, `read_fader`
/// logs per-window noise statistics and every latch emission over defmt/RTT so
/// noise can be characterized on a normally-operating device (probe attached).
/// Set to `false` for release builds — all diagnostic code compiles out.
const FADER_DIAG: bool = false;
/// Scan rounds per diagnostic stats window (64 rounds ≈ 1s at ~60Hz scan rate).
const FADER_DIAG_WINDOW_ROUNDS: u16 = 64;
/// Number of ADC reads taken within one mux dwell (~1ms). The MAX11300 sweep
/// refreshes the fader port's data register every sweep (~tens of µs), so each
/// read is a fresh conversion; the median of the burst rejects sweep glitches.
const FADER_BURST_READS: usize = 5;

type SharedMax = Mutex<NoopRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'static>>>;
type MuxPins = (
    Peri<'static, PIN_12>,
    Peri<'static, PIN_13>,
    Peri<'static, PIN_14>,
    Peri<'static, PIN_15>,
);

pub type MaxSender = Sender<'static, CriticalSectionRawMutex, MaxCmd, MAX_CHANNEL_SIZE>;
pub static MAX_CHANNEL: Channel<CriticalSectionRawMutex, MaxCmd, MAX_CHANNEL_SIZE> = Channel::new();

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_DAC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static MAX_VALUES_ADC: [AtomicU16; 20] = [const { AtomicU16::new(0) }; 20];
pub static CALIBRATING: AtomicBool = AtomicBool::new(false);

/// Set by `message_loop` when a port is reconfigured: the DAC data register may
/// have been rewritten (e.g. `gpo_configure_level` uses it for the GPO level),
/// so `process_channel_values` must not trust its last-written cache for that port.
static DAC_CACHE_DIRTY: [AtomicBool; 20] = [const { AtomicBool::new(false) }; 20];

#[derive(Clone)]
#[allow(dead_code)]
pub enum MaxCmd {
    ConfigurePort {
        port: Port,
        mode: Mode,
        gpo_level: Option<u16>,
    },
    GpoSetHigh {
        port: Port,
    },
    GpoSetLow {
        port: Port,
    },
    /// Drive these Mode3 ports high in a single SPI transaction. All bits
    /// in the same GPODAT register word latch simultaneously at the chip.
    GpoSetHighMany(heapless::Vec<Port, 4>),
    /// Drive these Mode3 ports low in a single SPI transaction.
    GpoSetLowMany(heapless::Vec<Port, 4>),
}

impl MaxCmd {
    /// Returns `true` if this command targets the fader port (`Port::P16`),
    /// which must never be reconfigured or driven as GPO while the fader task
    /// owns it. Callers can in theory construct a command with `Port::P16`,
    /// but `message_loop` drops such messages before touching the chip.
    fn touches_fader_port(&self) -> bool {
        match self {
            MaxCmd::ConfigurePort { port, .. }
            | MaxCmd::GpoSetHigh { port }
            | MaxCmd::GpoSetLow { port } => *port == Port::P16,
            MaxCmd::GpoSetHighMany(ports) | MaxCmd::GpoSetLowMany(ports) => {
                ports.contains(&Port::P16)
            }
        }
    }
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

    let mut fader_latches: [AnalogLatch; 16] = core::array::from_fn(|channel| {
        AnalogLatch::new(main_fader_values[channel], global_config.takeover_mode)
    });

    let mut chan: usize = 0;

    // Per-channel diagnostic stats over one window (see FADER_DIAG)
    let mut diag_min = [u16::MAX; 16];
    let mut diag_max = [0u16; 16];
    let mut diag_burst_spread = [0u16; 16];
    let mut diag_emits = [0u8; 16];
    let mut diag_publishes = [0u8; 16];
    let mut diag_rounds: u16 = 0;

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

        // Let the mux output settle, then take the burst back-to-back: each SPI
        // read spans several ADC sweep conversions, so the samples are already
        // decorrelated. A single timer await per dwell keeps the dwell short
        // (~1.3ms measured, ~45Hz scan rate); sleeping between reads would blow
        // it up to >2.4ms due to executor scheduling overshoot on the loaded core.
        Timer::after_micros(700).await;
        let mut burst = [0u16; FADER_BURST_READS];
        for slot in burst.iter_mut() {
            *slot = fader_port.get_value().await.unwrap();
        }
        let mut sorted = burst;
        sorted.sort_unstable();
        let val = sorted[FADER_BURST_READS / 2];

        if FADER_DIAG {
            diag_min[channel] = diag_min[channel].min(val);
            diag_max[channel] = diag_max[channel].max(val);
            let spread = sorted[FADER_BURST_READS - 1] - sorted[0];
            diag_burst_spread[channel] = diag_burst_spread[channel].max(spread);
        }

        // Scale a bit across the dead-zone (~4087 -> 4095) using integer math
        let val = (((val as u32 * 1002) / 1000) as u16).clamp(0, 4095);

        let latch = &mut fader_latches[channel];

        let target_value = match active_layer {
            LatchLayer::Main => main_fader_values[channel],
            LatchLayer::Alt => global_settings_fader_values[channel],
            LatchLayer::Third => 0,
        };

        if let Some(new_value) = latch.update(val, active_layer, target_value) {
            let diff = (new_value as i32 - target_value as i32).abs();

            if FADER_DIAG {
                diag_emits[channel] = diag_emits[channel].saturating_add(1);
                defmt::info!(
                    "FDIAG emit t={=u64}us ch={=usize} burst={} median={=u16} target={=u16} out={=u16} diff={=i32} latched={=bool}",
                    Instant::now().as_micros(),
                    channel,
                    burst,
                    val,
                    target_value,
                    new_value,
                    diff,
                    latch.is_latched()
                );
            }

            match active_layer {
                LatchLayer::Main => {
                    if diff >= 4 {
                        if FADER_DIAG {
                            diag_publishes[channel] = diag_publishes[channel].saturating_add(1);
                        }
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
                LatchLayer::Third => {}
            }
        }

        chan = (chan + 1) % 16;

        // End of a full scan round: flush the diagnostic window if due
        if FADER_DIAG && chan == 0 {
            diag_rounds += 1;
            if diag_rounds >= FADER_DIAG_WINDOW_ROUNDS {
                let mut range = [0u16; 16];
                for i in 0..16 {
                    range[i] = diag_max[i].saturating_sub(diag_min[i]);
                }
                defmt::info!(
                    "FDIAG win t={=u64}us range={} burst_spread={} emits={} pubs={}",
                    Instant::now().as_micros(),
                    range,
                    diag_burst_spread,
                    diag_emits,
                    diag_publishes
                );
                diag_min = [u16::MAX; 16];
                diag_max = [0u16; 16];
                diag_burst_spread = [0u16; 16];
                diag_emits = [0u8; 16];
                diag_publishes = [0u8; 16];
                diag_rounds = 0;
            }
        }
    }
}

#[embassy_executor::task]
async fn process_channel_values(
    max_driver: &'static SharedMax,
    calibration_data: Option<MaxCalibration>,
) {
    // Last calibrated value written to each DAC port. Rewriting an unchanged
    // value is a no-op at the chip, so those SPI writes are skipped to reduce
    // bus traffic and internal MAX11300 activity. The loop rate is unchanged.
    let mut last_written_dac: [Option<u16>; 20] = [None; 20];

    loop {
        // Hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;

        // Do not process channel 16 (faders)
        for i in (0..16).chain(17..20) {
            let port = Port::try_from(i).unwrap();
            let mut max = max_driver.lock().await;
            match max.get_mode(port) {
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

                    if DAC_CACHE_DIRTY[i].swap(false, Ordering::Relaxed) {
                        last_written_dac[i] = None;
                    }
                    if last_written_dac[i] != Some(calibrated_value) {
                        max.dac_set_value(port, calibrated_value).await.unwrap();
                        last_written_dac[i] = Some(calibrated_value);
                    }
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
        let msg = MAX_CHANNEL.receive().await;
        // The fader port (P16) is owned by `read_fader` and must never be
        // reconfigured or driven as GPO from here. Drop any command that
        // tries to touch it. See `MaxCmd::touches_fader_port` for details.
        if msg.touches_fader_port() {
            continue;
        }
        if let MaxCmd::ConfigurePort { port, .. } = &msg {
            DAC_CACHE_DIRTY[*port as usize].store(true, Ordering::Relaxed);
        }
        let mut max = max_driver.lock().await;

        match msg {
            MaxCmd::ConfigurePort {
                port,
                mode,
                gpo_level,
            } => match mode {
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
            MaxCmd::GpoSetHigh { port } => {
                max.gpo_set_high(port).await.unwrap();
            }
            MaxCmd::GpoSetLow { port } => {
                max.gpo_set_low(port).await.unwrap();
            }
            MaxCmd::GpoSetHighMany(ports) => {
                max.gpo_set_high_many(&ports).await.unwrap();
            }
            MaxCmd::GpoSetLowMany(ports) => {
                max.gpo_set_low_many(&ports).await.unwrap();
            }
        }
    }
}
