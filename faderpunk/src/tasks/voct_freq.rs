//! AUX-input frequency measurement for the V/Oct calibration wizard.
//!
//! The three AUX pins are owned by the clock task; `aux_pin_loop` time-shares
//! each pin between clock-edge detection (`clock::make_ext_clock_loop`) and
//! frequency measurement, switching over when a measurement is requested.

use embassy_futures::select::select;
use embassy_rp::gpio::Input;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{with_timeout, Duration, Instant};

use libfp::ClockSrc;

use crate::tasks::clock::make_ext_clock_loop;

/// Signals the AUX pin loop to pause clock detection and run a frequency
/// measurement. Index corresponds to aux pin: 0=Atom, 1=Meteor, 2=Cube.
pub static VOCT_MEASURE_REQ: [Signal<CriticalSectionRawMutex, ()>; 3] =
    [const { Signal::new() }; 3];
/// Result of a V/Oct frequency measurement: Ok(freq_hz) or Err(()) on timeout.
pub static VOCT_MEASURE_RES: Signal<CriticalSectionRawMutex, Result<f32, ()>> = Signal::new();

/// Measure the frequency of an incoming signal on `pin` by timing rising edges.
/// Returns `Ok(freq_hz)` or `Err(())` on timeout (2 s without a valid edge).
/// Accepts only signals in the 10 Hz – 10 kHz band; edges outside this range
/// are discarded without corrupting the period accumulator.
///
/// 256 samples span ~580 ms at 440 Hz, keeping endpoint jitter below 0.01%.
async fn sync_to_rising_edge(pin: &mut Input<'_>, timeout: Duration) -> Result<Instant, ()> {
    loop {
        with_timeout(timeout, pin.wait_for_rising_edge())
            .await
            .map_err(|_| ())?;
        let now = Instant::now();
        // Level-check: if pin is already LOW the edge was a narrow glitch that
        // resolved before Embassy scheduled us. Skip it without advancing the
        // anchor — mirrors little-helper's ISR-level pin re-read.
        if pin.is_high() {
            return Ok(now);
        }
    }
}

async fn measure_frequency(pin: &mut Input<'_>) -> Result<f32, ()> {
    const ROUGH_SAMPLES: u64 = 8;
    const FINE_SAMPLES: u64 = 256;
    const MIN_PERIOD_US: u64 = 100; // 10 kHz upper limit
    const MAX_PERIOD_US: u64 = 100_000; // 10 Hz lower limit
    const TIMEOUT: Duration = Duration::from_secs(2);

    // --- Phase 1: rough period estimate (wide gate, few samples) ---
    let mut last_edge = sync_to_rising_edge(pin, TIMEOUT).await?;
    let mut rough_total_us: u64 = 0;
    let mut rough_valid: u64 = 0;

    while rough_valid < ROUGH_SAMPLES {
        let now = sync_to_rising_edge(pin, TIMEOUT).await?;
        let period_us = now.duration_since(last_edge).as_micros();

        if period_us < MIN_PERIOD_US {
            continue;
        }
        if period_us > MAX_PERIOD_US {
            last_edge = now;
            continue;
        }
        last_edge = now;
        rough_total_us += period_us;
        rough_valid += 1;
    }

    if rough_total_us == 0 {
        return Err(());
    }
    let rough_period_us = rough_total_us / rough_valid;

    // --- Phase 2: fine measurement with adaptive gate ---
    // Gate rejects any interval outside [60%, 150%] of the rough period.
    // A spurious edge mid-cycle creates a sub-fundamental interval that sits
    // below the 60% floor and is rejected without advancing the anchor.
    let lo = rough_period_us * 6 / 10;
    let hi = rough_period_us * 15 / 10;

    last_edge = sync_to_rising_edge(pin, TIMEOUT).await?;
    let mut total_period_us: u64 = 0;
    let mut valid: u64 = 0;

    while valid < FINE_SAMPLES {
        let now = sync_to_rising_edge(pin, TIMEOUT).await?;
        let period_us = now.duration_since(last_edge).as_micros();

        if period_us < lo {
            // Short interval: spurious edge — keep current anchor.
            continue;
        }
        if period_us > hi {
            if period_us > MAX_PERIOD_US {
                // Gap / sub-10 Hz: reset anchor.
                last_edge = now;
            }
            continue;
        }

        last_edge = now;
        total_period_us += period_us;
        valid += 1;
    }

    if total_period_us == 0 {
        return Err(());
    }
    Ok(FINE_SAMPLES as f32 * 1_000_000.0 / total_period_us as f32)
}

/// Runs the clock-detection loop for one AUX pin, breaking out to perform a
/// frequency measurement whenever `VOCT_MEASURE_REQ[aux_idx]` is signalled.
pub async fn aux_pin_loop(mut pin: Input<'_>, src: ClockSrc, aux_idx: usize) {
    loop {
        select(
            make_ext_clock_loop(&mut pin, src),
            VOCT_MEASURE_REQ[aux_idx].wait(),
        )
        .await;

        // Measurement requested: measure and signal result; clock loop restarts.
        let result = measure_frequency(&mut pin).await;
        VOCT_MEASURE_RES.signal(result);
    }
}

/// Debug helper: continuously measures frequency on `pin` and logs each result.
/// First pass also logs raw per-edge periods to diagnose spurious edges.
/// Swap into `run_clock_sources` in place of `aux_pin_loop` for field testing.
#[allow(dead_code)]
async fn debug_freq_loop(mut pin: Input<'_>) {
    const TIMEOUT: Duration = Duration::from_secs(2);
    const MIN_PERIOD_US: u64 = 100;
    const MAX_PERIOD_US: u64 = 100_000;

    defmt::info!("ATOM debug: logging raw periods for first 20 edges");
    let Ok(mut last_edge) = sync_to_rising_edge(&mut pin, TIMEOUT).await else {
        defmt::warn!("ATOM debug: no signal on startup");
        return;
    };
    let mut rejected: u32 = 0;
    let mut accepted: u32 = 0;
    let mut min_us: u64 = u64::MAX;
    let mut max_us: u64 = 0;

    while accepted < 20 {
        let Ok(now) = sync_to_rising_edge(&mut pin, TIMEOUT).await else {
            defmt::warn!("ATOM debug: timeout during raw period log");
            break;
        };
        let period_us = now.duration_since(last_edge).as_micros();
        if !(MIN_PERIOD_US..=MAX_PERIOD_US).contains(&period_us) {
            rejected += 1;
            last_edge = now;
            continue;
        }
        defmt::info!("ATOM raw period: {} us", period_us);
        if period_us < min_us {
            min_us = period_us;
        }
        if period_us > max_us {
            max_us = period_us;
        }
        last_edge = now;
        accepted += 1;
    }
    defmt::info!(
        "ATOM raw: {} accepted, {} rejected, min={} us, max={} us",
        accepted,
        rejected,
        min_us,
        max_us
    );

    defmt::info!("ATOM debug: starting continuous frequency measurement");
    loop {
        defmt::info!("ATOM debug: waiting for signal...");
        match measure_frequency(&mut pin).await {
            Ok(hz) => defmt::info!("ATOM freq: {} Hz", hz),
            Err(()) => defmt::warn!("ATOM: no signal (timeout after 2s)"),
        }
    }
}
