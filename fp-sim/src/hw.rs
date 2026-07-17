//! Virtual hardware: stands in for the MAX11300 driver and the LED strip so
//! the shared channels always have a consumer, and tracks the state the
//! panel UI displays (port modes, gate levels, rendered LED colors).

use embassy_time::Timer;
use max11300::config::Mode;
use portable_atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use fp_core::tasks::leds::{LedProcessor, LED_BRIGHTNESS, NUM_LEDS, T};
use fp_core::tasks::max::{MaxCmd, MAX_CHANNEL};

/// What a port is currently configured as, mirroring the MAX11300 mode it
/// was given via `MaxCmd::ConfigurePort`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PortMode {
    /// Mode 0 — high impedance / unused
    Unconfigured = 0,
    /// Mode 3 — GPO (gate output)
    GateOut = 3,
    /// Mode 5 — DAC (CV output)
    CvOut = 5,
    /// Mode 7 — ADC (CV input)
    CvIn = 7,
}

/// Voltage range of a configured CV port.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PortRange {
    /// 0..10V
    Unipolar = 0,
    /// -5..5V
    Bipolar = 1,
}

/// Per-port mode (`PortMode` as u8) for the 16 channel jacks + 4 aux ports.
pub static PORT_MODES: [AtomicU8; 20] = [const { AtomicU8::new(0) }; 20];
/// Per-port voltage range (`PortRange` as u8); only meaningful for CV modes.
pub static PORT_RANGES: [AtomicU8; 20] = [const { AtomicU8::new(0) }; 20];
/// Gate levels for Mode3 (GPO) ports.
pub static GATE_STATES: [AtomicBool; 20] = [const { AtomicBool::new(false) }; 20];

/// Rendered LED frame, one `0x00RRGGBB` word per LED, brightness applied.
/// Written by [`run_leds`] at the hardware refresh rate, read by the UI.
pub static LED_FRAME: [AtomicU32; NUM_LEDS] = [const { AtomicU32::new(0) }; NUM_LEDS];

pub fn port_mode(port: usize) -> PortMode {
    match PORT_MODES[port].load(Ordering::Relaxed) {
        3 => PortMode::GateOut,
        5 => PortMode::CvOut,
        7 => PortMode::CvIn,
        _ => PortMode::Unconfigured,
    }
}

pub fn port_range(port: usize) -> PortRange {
    match PORT_RANGES[port].load(Ordering::Relaxed) {
        1 => PortRange::Bipolar,
        _ => PortRange::Unipolar,
    }
}

/// Consumes MAX11300 commands and mirrors their effect into the shared
/// state above. DAC/ADC values already flow through `MAX_VALUES_DAC/ADC`.
#[embassy_executor::task]
pub async fn run_virtual_max() {
    loop {
        match MAX_CHANNEL.receive().await {
            MaxCmd::ConfigurePort { port, mode, .. } => {
                let port = port as usize;
                let (mode_no, range) = match mode {
                    Mode::Mode3(_) => {
                        // The firmware drives freshly configured gates high
                        GATE_STATES[port].store(true, Ordering::Relaxed);
                        (PortMode::GateOut, PortRange::Unipolar)
                    }
                    Mode::Mode5(config) => (
                        PortMode::CvOut,
                        match config.0 {
                            max11300::config::DACRANGE::RgNeg5_5v => PortRange::Bipolar,
                            _ => PortRange::Unipolar,
                        },
                    ),
                    Mode::Mode7(config) => (
                        PortMode::CvIn,
                        match config.1 {
                            max11300::config::ADCRANGE::RgNeg5_5v => PortRange::Bipolar,
                            _ => PortRange::Unipolar,
                        },
                    ),
                    _ => (PortMode::Unconfigured, PortRange::Unipolar),
                };
                PORT_MODES[port].store(mode_no as u8, Ordering::Relaxed);
                PORT_RANGES[port].store(range as u8, Ordering::Relaxed);
                log::debug!("MAX: configure port {port}");
            }
            MaxCmd::GpoSetHigh { port } => {
                GATE_STATES[port as usize].store(true, Ordering::Relaxed);
            }
            MaxCmd::GpoSetLow { port } => {
                GATE_STATES[port as usize].store(false, Ordering::Relaxed);
            }
            MaxCmd::GpoSetHighMany(ports) => {
                for port in ports {
                    GATE_STATES[port as usize].store(true, Ordering::Relaxed);
                }
            }
            MaxCmd::GpoSetLowMany(ports) => {
                for port in ports {
                    GATE_STATES[port as usize].store(false, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Headless-mode helper: periodically prints channel 0's fader and CV
/// output values.
#[embassy_executor::task]
pub async fn dac_monitor() {
    use fp_core::tasks::clock::TICK_COUNTER;
    use fp_core::tasks::max::{MAX_VALUES_DAC, MAX_VALUES_FADER};

    loop {
        Timer::after_millis(250).await;
        let fader = MAX_VALUES_FADER[0].load(Ordering::Relaxed);
        let dac = MAX_VALUES_DAC[0].load(Ordering::Relaxed);
        let ticks = TICK_COUNTER.load(Ordering::Relaxed);
        log::info!(
            "ch0: fader={fader:4} dac={dac:4} ticks={ticks:6} {}",
            bar(dac)
        );
    }
}

fn bar(value: u16) -> String {
    const WIDTH: usize = 32;
    let filled = (value as usize * WIDTH) / 4096;
    let mut s = String::with_capacity(WIDTH);
    for i in 0..WIDTH {
        s.push(if i < filled { '█' } else { '·' });
    }
    s
}

/// Runs the LED effect engine at the hardware refresh rate and publishes
/// each rendered frame (with the global brightness applied, like the WS2812
/// task does) into [`LED_FRAME`] for the UI.
#[embassy_executor::task]
pub async fn run_leds() {
    let mut leds = LedProcessor::new();
    loop {
        Timer::after_millis(T).await;
        leds.poll_messages();
        let frame = leds.render();
        let brightness = LED_BRIGHTNESS.load(Ordering::Relaxed) as u32 + 1;
        for (led, color) in LED_FRAME.iter().zip(frame.iter()) {
            let r = (color.r as u32 * brightness) >> 8;
            let g = (color.g as u32 * brightness) >> 8;
            let b = (color.b as u32 * brightness) >> 8;
            led.store((r << 16) | (g << 8) | b, Ordering::Relaxed);
        }
    }
}
