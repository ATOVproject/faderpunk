//! Virtual hardware: stands in for the MAX11300 driver and the LED strip so
//! the shared channels always have a consumer.

use embassy_time::Timer;
use portable_atomic::Ordering;

use fp_core::tasks::clock::TICK_COUNTER;
use fp_core::tasks::leds::{LedProcessor, T};
use fp_core::tasks::max::{MaxCmd, MAX_CHANNEL, MAX_VALUES_DAC, MAX_VALUES_FADER};

/// Drains MAX11300 commands. Port reconfigurations and gate levels are only
/// logged for now; DAC/ADC values already flow through the shared atomics.
#[embassy_executor::task]
pub async fn run_virtual_max() {
    loop {
        match MAX_CHANNEL.receive().await {
            MaxCmd::ConfigurePort { port, .. } => {
                log::debug!("MAX: configure port {}", port as usize);
            }
            MaxCmd::GpoSetHigh { port } => {
                log::debug!("MAX: gate high on port {}", port as usize);
            }
            MaxCmd::GpoSetLow { port } => {
                log::debug!("MAX: gate low on port {}", port as usize);
            }
            MaxCmd::GpoSetHighMany(ports) => {
                log::debug!("MAX: {} gates high", ports.len());
            }
            MaxCmd::GpoSetLowMany(ports) => {
                log::debug!("MAX: {} gates low", ports.len());
            }
        }
    }
}

/// Runs the LED effect engine at the hardware refresh rate. Headless for now:
/// rendering keeps effect state moving and drains the overlay channel; a
/// panel UI will consume the rendered buffer later.
#[embassy_executor::task]
pub async fn run_leds() {
    let mut leds = LedProcessor::new();
    loop {
        Timer::after_millis(T).await;
        leds.poll_messages();
        let _ = leds.render();
    }
}

/// PoC scope: periodically prints channel 0's fader and CV output values.
#[embassy_executor::task]
pub async fn dac_monitor() {
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
