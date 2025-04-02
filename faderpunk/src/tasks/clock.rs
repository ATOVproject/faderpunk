use embassy_futures::{
    join::join5,
    select::{select, Either},
};
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::{Channel, Receiver},
    mutex::Mutex,
    watch::Sender,
};
use embassy_time::{with_timeout, Duration, Instant, Ticker};

use crate::{Spawner, CLOCK_WATCH, WATCH_CONFIG_CHANGE};
use config::ClockSrc;
use libfp::utils::bpm_to_clock_duration;

type AuxInputs = (PIN_1, PIN_2, PIN_3);

// Multiplier for dynamic timeout based on last interval
const TIMEOUT_MULTIPLIER: u32 = 4;
// Maximum timeout - ensures detection of slow clocks after a stop.
// Based on slowest supported clock: 20 BPM @ 1 PPQN = 3000ms interval. Set higher.
const MAX_TIMEOUT: Duration = Duration::from_secs(4);
// Minimum timeout - prevents jitter on fast clocks causing false timeouts.
// Needs tuning, maybe 50ms-100ms? Let's try 100ms.
const MIN_TIMEOUT: Duration = Duration::from_millis(100);

// Optional: Minimum interval to debounce noise, based on fastest supported clock:
// 300 BPM @ 24 PPQN = 8.33ms interval. Set debounce lower, e.g., 3ms.
const MIN_INTERVAL: Duration = Duration::from_millis(3);

// This constant should still match the ACTUAL PPQN of the currently connected source
// It's used by Task B (Generator) via the ClockCommand::SetInterval payload, implicitly.
// Task A itself doesn't strictly need it unless doing PPQN-specific checks here.
const SOURCE_PPQN: u32 = 2; // Example - configure as needed

const TARGET_PPQN: u32 = 24;
// Pre-calculate the ratio
const OUTPUT_TICKS_PER_INPUT_TICK: u32 = TARGET_PPQN / SOURCE_PPQN;

// Command sent from Detector (A) to Generator (B)
#[derive(Clone, Copy, Debug)] // Make it easily copyable
pub enum ClockCommand {
    /// Set the desired interval for the *input* clock's pulse.
    /// The generator will derive the output tick duration from this.
    SetInterval(Duration),
    /// Indicates the input clock has stopped (timeout detected).
    ClockStopped,
}

// TODO: Instead of is_reset, have clock commands, reset/stop, tick

// Choose a small channel size, e.g., 2-4. Task B should process quickly.
// TODO: Make this 4 again
const COMMAND_CHANNEL_SIZE: usize = 128;
// Define a static channel - requires embassy-sync feature "static-channel" or manual init
static CLOCK_COMMAND_CHANNEL: Channel<ThreadModeRawMutex, ClockCommand, COMMAND_CHANNEL_SIZE> =
    Channel::new();

pub async fn start_clock(
    spawner: &Spawner,
    aux_inputs: AuxInputs,
    receiver: Receiver<'static, NoopRawMutex, f32, 64>,
) {
    // spawner.spawn(run_clock(aux_inputs, receiver)).unwrap();
    spawner.spawn(detector_task(aux_inputs)).unwrap();
    spawner.spawn(generator_task()).unwrap();
}
// TODO: usability: DO store the last interval even when the clock stops.
// It is usually very likely that the clock doesn't change in between start/stops

// TODO: We _could_ dynamically spawn this task if external clock is configured
// Also then we could just pass that one pin
#[embassy_executor::task]
pub async fn detector_task(
    // Pass necessary hardware, like the input pin
    aux_pins: AuxInputs,
) {
    let mut last_instant: Option<Instant> = None;
    let mut last_measured_interval: Option<Duration> = None;
    let sender = CLOCK_COMMAND_CHANNEL.sender();
    let (atom_pin, meteor_pin, hexagon_pin) = aux_pins;
    let mut atom = Input::new(atom_pin, Pull::Up);

    defmt::info!("Detector Task started.");

    loop {
        let current_timeout = match last_measured_interval {
            Some(last_interval) => {
                // Calculate dynamic timeout
                let calculated_timeout = last_interval * TIMEOUT_MULTIPLIER;
                // Clamp between min and max bounds
                calculated_timeout.clamp(MIN_TIMEOUT, MAX_TIMEOUT)
            }
            None => MAX_TIMEOUT,
        };

        let pin_fut = async {
            atom.wait_for_falling_edge().await;
            atom.wait_for_low().await;
        };

        let pin_result = with_timeout(current_timeout, pin_fut).await;

        match pin_result {
            Ok(()) => {
                // Clock Tick Detected
                let now = Instant::now();

                if let Some(last) = last_instant {
                    let interval = now - last;
                    last_instant = Some(now);

                    // Debounce
                    if interval < MIN_INTERVAL {
                        continue;
                    }

                    // TODO: consolidate
                    last_measured_interval = Some(interval);

                    sender.try_send(ClockCommand::SetInterval(interval)).ok();
                    // let bpm = (60.0 * 1_000_000.0)
                    //     / (interval.as_micros() * INPUT_PPQN_INT as u64) as f64;
                    // defmt::info!("BPM {}", bpm);
                } else {
                    // This is the first tick after startup OR after a timeout/stop
                    last_instant = Some(now);
                    let assumed_interval =
                        // 120BPM
                        last_measured_interval.unwrap_or(Duration::from_nanos(20_833_333));
                    sender
                        .try_send(ClockCommand::SetInterval(assumed_interval))
                        .ok();
                }
            }
            Err(_) => {
                // Clock probably stopped
                if last_instant.is_some() {
                    // Reset everything
                    last_instant = None;
                    last_measured_interval = None;
                    // Tell the generator task the clock stopped
                    sender.try_send(ClockCommand::ClockStopped).ok();
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn generator_task() {
    let receiver = CLOCK_COMMAND_CHANNEL.receiver();
    let clock_sender = CLOCK_WATCH.sender(); // Assuming this is your output mechanism
    let mut ticker: Option<Ticker> = None;
    let mut ticks_sent_this_cycle: u32 = 0;

    defmt::info!("Generator Task started.");

    loop {
        let cmd_fut = receiver.receive();
        let tick_fut = async {
            if let Some(t) = &mut ticker {
                t.next().await;
            } else {
                core::future::pending::<()>().await;
            }
        };

        match select(cmd_fut, tick_fut).await {
            Either::First(cmd) => {
                match cmd {
                    ClockCommand::SetInterval(input_interval) => {
                        defmt::trace!(
                            "Generator: Received SetInterval: {} ms",
                            input_interval.as_millis()
                        );
                        if OUTPUT_TICKS_PER_INPUT_TICK > 0 {
                            // ++ Reverted to integer division ++
                            let output_tick_duration = input_interval / OUTPUT_TICKS_PER_INPUT_TICK;
                            // ----------------------------------

                            if output_tick_duration.as_micros() > 0 {
                                // Send Tick 1 immediately
                                defmt::trace!("Generator: Sending Tick 1 (immediate)");
                                clock_sender.send(false); // Assuming non-blocking update

                                // Reset counter for the new cycle
                                ticks_sent_this_cycle = 1;

                                // Create ticker for subsequent ticks using truncated duration
                                ticker = Some(Ticker::every(output_tick_duration));
                                defmt::info!(
                                    "Generator: Ticker (re)started (duration: {} us)",
                                    output_tick_duration.as_micros()
                                );
                            } else {
                                ticker = None;
                                ticks_sent_this_cycle = 0; // Reset counter
                                defmt::warn!("Generator: Calculated output tick duration is zero. Ticker stopped.");
                            }
                        } else {
                            ticker = None;
                            ticks_sent_this_cycle = 0; // Reset counter
                            defmt::error!("Generator: OUTPUT_TICKS_PER_INPUT_TICK is zero!");
                        }
                    }
                    ClockCommand::ClockStopped => {
                        defmt::warn!("Generator: Received ClockStopped command. Ticker stopped.");
                        ticker = None;
                        ticks_sent_this_cycle = 0; // Reset counter
                    }
                }
            }
            Either::Second(_) => {
                // --- Ticker Fired ---
                if ticks_sent_this_cycle > 0 && ticks_sent_this_cycle < OUTPUT_TICKS_PER_INPUT_TICK
                {
                    // This represents Ticks 2, 3, ... N
                    defmt::trace!(
                        "Generator: Tick {} (subsequent)!",
                        ticks_sent_this_cycle + 1
                    );
                    clock_sender.send(false); // Assuming non-blocking update
                    ticks_sent_this_cycle += 1;

                    // Optional: Stop ticker after sending the last one
                    if ticks_sent_this_cycle >= OUTPUT_TICKS_PER_INPUT_TICK {
                        defmt::trace!("Generator: Sent final tick for interval, stopping ticker.");
                        ticker = None;
                    }
                } else {
                    // Ticker fired but we've already sent N ticks OR clock is stopped (counter is 0).
                    defmt::trace!(
                        "Generator: Ignoring spurious ticker fire (Sent {}/{}).",
                        ticks_sent_this_cycle,
                        OUTPUT_TICKS_PER_INPUT_TICK
                    );
                    if ticks_sent_this_cycle >= OUTPUT_TICKS_PER_INPUT_TICK {
                        ticker = None;
                    }
                }
            }
        }
    }
}

// async fn make_ext_clock_loop(
//     mut pin: Input<'_>,
//     clock_src: ClockSrc,
//     clock_sender: Sender<'static, CriticalSectionRawMutex, bool, 16>,
// ) {
//     // let mut config_receiver = WATCH_CONFIG_CHANGE.receiver().unwrap();
//     // let mut current_config = config_receiver.get().await;
//
//     let mut last_instant: Option<Instant> = None;
//
//     loop {
//         // 2. Main detection loop
//         pin.wait_for_falling_edge().await;
//         pin.wait_for_low().await; // 3. Wait for input clock edge (falling)
//                                   // defmt::info!("SENDING TICK 1");
//         clock_sender.send(false); // 4. Send the corresponding output tick immediately
//         let now = Instant::now(); // 5. Record the time of the input tick
//
//         if let Some(last) = last_instant {
//             // 6. Check if we have a previous tick time
//             // This block executes from the SECOND input tick onwards
//
//             last_instant = Some(now); // 7. IMPORTANT: Update last_instant for the *next* iteration
//
//             let interval = now - last; // 8. Calculate duration since the previous input tick
//             defmt::info!("INTERVAL: {}", interval.as_micros());
//
//             // 9. Calculate BPM (assuming input is 4 PPQN) - logically correct based on formula
//             // let bpm = (60.0 * 1_000_000.0) / (interval.as_micros() as f32 * 4.0);
//             // defmt::info!("BPM: {}", bpm);
//
//             // TODO: smoothing - Acknowledged, okay for now
//             // IMPORTANT: Only support external ppqn that are divisors of 24 - Note okay
//             // interval / (ppqn_int / ppqn_ext) - Useful comment for generalization
//
//             // 10. Calculate Ticker duration for interpolated ticks
//             // GOAL: Generate 24 PPQN output from 4 PPQN input.
//             //       Means 24/4 = 6 output ticks per input interval.
//             //       One tick was already sent (step 4). Need 5 more.
//             //       These 6 ticks should span the 'interval' duration.
//             //       So, the duration between each of the 6 output ticks should be 'interval / 6'.
//             let ticker_duration = interval / (24 / 4); // Should be interval / 6
//                                                        // !!! Your code has `interval / 24 / 4` which is `interval / 96`. This seems INCORRECT.
//                                                        // !!! Assuming you meant `interval / (24 / 4)` for the logic below.
//             let mut t = Ticker::every(ticker_duration); // Create ticker with duration = interval / 6
//
//             let mut i = 1; // 11. Initialize counter for *additional* ticks
//             loop {
//                 // 12. Inner loop to generate interpolated ticks
//                 // We need 5 more ticks (since tick 1 was already sent).
//                 // This loop should run 5 times for i = 1, 2, 3, 4, 5.
//                 if i == (24 / 4) {
//                     // Check if i == 6. Exit before sending the 6th additional tick.
//                     break; // Correct break condition to generate 5 more ticks (i=1 to 5)
//                 }
//                 t.next().await; // 13. Wait for the sub-interval duration
//                                 // defmt::info!("SENDING TICK {}", i + 1); // Logs ticks 2, 3, 4, 5, 6
//                 clock_sender.send(false); // 14. Send the interpolated tick
//                 i += 1; // Increment counter
//             }
//             // After loop: Sent Tick 1 + Ticks 2, 3, 4, 5, 6 = Total 6 ticks for the interval. Correct.
//         } else {
//             // This block executes only for the VERY FIRST input tick
//             last_instant = Some(now); // 15. Store the time of the first tick
//         }
//     }
//
//     // loop {
//     //     let should_be_active =
//     //         current_config.clock_src == clock_src || current_config.reset_src == clock_src;
//     //
//     //     if !should_be_active {
//     //         current_config = config_receiver.changed().await;
//     //         // Re-check active condition with new config
//     //         continue;
//     //     }
//     //
//     //     // TODO: Config here changes only after a tick, we need to use select
//     //     pin.wait_for_falling_edge().await;
//     //     pin.wait_for_low().await;
//     //
//     //     clock_sender.send(current_config.reset_src == clock_src);
//     //
//     //     // Check if config has changed after waiting
//     //     if let Some(new_config) = config_receiver.try_get() {
//     //         current_config = new_config;
//     //     }
//     // }
// }
//
// // TODO: read config from eeprom and pass in config object
// #[embassy_executor::task]
// async fn run_clock(aux_inputs: AuxInputs, receiver: Receiver<'static, NoopRawMutex, f32, 64>) {
//     let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
//     let atom = Input::new(atom_pin, Pull::Up);
//     let meteor = Input::new(meteor_pin, Pull::Up);
//     let cube = Input::new(hexagon_pin, Pull::Up);
//     let clock_sender = CLOCK_WATCH.sender();
//     // TODO: Get PPQN from config somehow (and keep updated)
//     const PPQN: u8 = 24;
//
//     // TODO: get ms AND ppqn from eeprom (or config somehow??!)
//     let internal_clock: Mutex<NoopRawMutex, Ticker> =
//         Mutex::new(Ticker::every(bpm_to_clock_duration(120.0, PPQN)));
//
//     let internal_fut = async {
//         let mut config_receiver = WATCH_CONFIG_CHANGE.receiver().unwrap();
//         let mut current_config = config_receiver.get().await;
//
//         loop {
//             // TODO: How to handle internal reset?
//             // IDEA: Clocking apps can reset
//             let should_be_active = current_config.clock_src == ClockSrc::Internal;
//
//             if !should_be_active {
//                 current_config = config_receiver.changed().await;
//                 // Re-check active condition with new config
//                 continue;
//             }
//
//             // TODO: Config here changes only after a tick, we need to use select
//             let mut clock = internal_clock.lock().await;
//             clock.next().await;
//
//             clock_sender.send(false);
//
//             // Check if config has changed after waiting
//             if let Some(new_config) = config_receiver.try_get() {
//                 current_config = new_config;
//             }
//         }
//     };
//
//     let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom, clock_sender.clone());
//     let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor, clock_sender.clone());
//     let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube, clock_sender.clone());
//
//     let msg_fut = async {
//         loop {
//             let bpm = receiver.receive().await;
//             let mut clock = internal_clock.lock().await;
//             *clock = Ticker::every(bpm_to_clock_duration(bpm, PPQN));
//         }
//     };
//
//     join5(internal_fut, atom_fut, meteor_fut, cube_fut, msg_fut).await;
// }
