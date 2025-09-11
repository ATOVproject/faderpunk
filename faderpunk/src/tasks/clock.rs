use core::sync::atomic::Ordering;

use defmt::info;
use embassy_futures::{
    join::join4,
    select::{select, select3, Either, Either3},
};
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_1, PIN_2, PIN_3},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Channel,
    pubsub::{PubSubChannel, Subscriber},
};
use embassy_time::{Instant, Timer};

use libfp::{utils::bpm_to_clock_duration, AuxJackMode, ClockSrc, GlobalConfig};
use midly::live::{LiveEvent, SystemRealtime};

use crate::{
    tasks::{max::MAX_TRIGGERS_GPO, midi::MIDI_CHANNEL},
    Spawner, GLOBAL_CONFIG_WATCH,
};

const CLOCK_PUBSUB_SIZE: usize = 16;
// 16 apps
const CLOCK_PUBSUB_SUBSCRIBERS: usize = 16;
// 3 Ext clocks, internal clock, midi
const CLOCK_PUBSUB_PUBLISHERS: usize = 5;

type AuxInputs = (
    Peri<'static, PIN_1>,
    Peri<'static, PIN_2>,
    Peri<'static, PIN_3>,
);
pub type ClockSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    ClockEvent,
    CLOCK_PUBSUB_SIZE,
    CLOCK_PUBSUB_SUBSCRIBERS,
    CLOCK_PUBSUB_PUBLISHERS,
>;

pub static CLOCK_PUBSUB: PubSubChannel<
    CriticalSectionRawMutex,
    ClockEvent,
    CLOCK_PUBSUB_SIZE,
    CLOCK_PUBSUB_SUBSCRIBERS,
    CLOCK_PUBSUB_PUBLISHERS,
> = PubSubChannel::new();

pub static CLOCK_CMD_CHANNEL: Channel<ThreadModeRawMutex, ClockCmd, 8> = Channel::new();

#[derive(Clone, Copy)]
pub enum ClockCmd {
    Start,
    Stop,
    Toggle,
}

#[derive(Clone, Copy)]
pub enum ClockEvent {
    Tick,
    Start,
    Reset,
}

pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock(aux_inputs)).unwrap();
}

async fn make_ext_clock_loop(mut pin: Input<'_>, clock_src: ClockSrc) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let mut current_config = config_receiver.get().await;
    let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();

    loop {
        let should_be_active = current_config.clock.clock_src == clock_src
            || current_config.clock.reset_src == clock_src;

        if !should_be_active {
            current_config = config_receiver.changed().await;
            // Re-check active condition with new config
            continue;
        }

        match select(pin.wait_for_falling_edge(), config_receiver.changed()).await {
            Either::First(()) => {
                // Pin event happened.
                pin.wait_for_low().await;

                let clock_event = if current_config.clock.reset_src == clock_src {
                    ClockEvent::Reset
                } else {
                    ClockEvent::Tick
                };

                clock_publisher.publish(clock_event).await;
            }
            Either::Second(new_config) => {
                // Config change happened.
                current_config = new_config;
            }
        }
    }
}

fn send_analog_ticks(spawner: &Spawner, config: &GlobalConfig, counts: &[u8; 3]) {
    for (i, aux_jack) in config.aux.iter().enumerate() {
        if let AuxJackMode::ClockOut(div) = aux_jack {
            if counts[i] % *div as u8 == 0 {
                spawner.spawn(analog_tick(17 + i)).unwrap();
            }
        }
    }
}

#[embassy_executor::task]
async fn analog_tick(gpo_index: usize) {
    // Set the output high (ramp up).
    MAX_TRIGGERS_GPO[gpo_index].store(2, Ordering::Relaxed);

    // Wait for the pulse width. 10ms is a common value for triggers.
    Timer::after_millis(10).await;

    // Set the output low (ramp down).
    MAX_TRIGGERS_GPO[gpo_index].store(1, Ordering::Relaxed);
}

#[embassy_executor::task]
async fn run_clock(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Down);
    let meteor = Input::new(meteor_pin, Pull::Down);
    let cube = Input::new(hexagon_pin, Pull::Down);
    let spawner = Spawner::for_current_executor().await;

    let internal_fut = async {
        let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
        let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();
        let clock_receiver = CLOCK_CMD_CHANNEL.receiver();

        let mut config = config_receiver.get().await;
        let mut tick_duration =
            bpm_to_clock_duration(config.clock.internal_bpm, config.clock.ext_ppqn);
        let mut next_tick_at = Instant::now();
        let mut is_running = false;
        // These are for clock divisions
        let mut analog_out_counts: [u8; 3] = [0; 3];

        loop {
            // TODO: How to handle internal reset?
            let should_be_active = is_running && config.clock.clock_src == ClockSrc::Internal;

            // This future only resolves on a tick when the internal clock is active.
            // Otherwise, it pends forever, preventing ticks from being generated.
            let tick_fut = async {
                if should_be_active {
                    Timer::at(next_tick_at).await;
                } else {
                    core::future::pending::<()>().await;
                }
            };

            match select3(
                tick_fut,
                config_receiver.changed(),
                clock_receiver.receive(),
            )
            .await
            {
                Either3::First(_) => {
                    // Schedule next tick relative to the previous one to avoid drift.
                    next_tick_at += tick_duration;
                    clock_publisher.publish(ClockEvent::Tick).await;
                    MIDI_CHANNEL
                        .send(LiveEvent::Realtime(SystemRealtime::TimingClock))
                        .await;
                    send_analog_ticks(&spawner, &config, &analog_out_counts);
                    // Increment counters for the *next* tick.
                    for count in analog_out_counts.iter_mut() {
                        *count = count.wrapping_add(1);
                    }
                }
                Either3::Second(new_config) => {
                    let old_tick_duration = tick_duration;
                    let new_tick_duration = bpm_to_clock_duration(
                        new_config.clock.internal_bpm,
                        new_config.clock.ext_ppqn,
                    );

                    // If BPM changes while the clock is running, adjust the next tick time
                    // to make the transition smooth.
                    if old_tick_duration != new_tick_duration && should_be_active {
                        let now = Instant::now();
                        if let Some(time_until_next_tick) = next_tick_at.checked_duration_since(now)
                        {
                            if old_tick_duration.as_ticks() > 0 {
                                let new_time_until_next_tick_ticks =
                                    (time_until_next_tick.as_ticks() as u128
                                        * new_tick_duration.as_ticks() as u128)
                                        / old_tick_duration.as_ticks() as u128;
                                let new_time_until_next_tick = embassy_time::Duration::from_ticks(
                                    new_time_until_next_tick_ticks as u64,
                                );
                                next_tick_at = now + new_time_until_next_tick;
                            }
                        }
                    }

                    config = new_config;
                    tick_duration = new_tick_duration;
                }
                Either3::Third(cmd) => {
                    if config.clock.clock_src == ClockSrc::Internal {
                        let next_is_running = match cmd {
                            ClockCmd::Start => true,
                            ClockCmd::Stop => false,
                            ClockCmd::Toggle => !is_running,
                        };

                        if !is_running && next_is_running {
                            // Reset analog clock phase on start
                            analog_out_counts = [0; 3];

                            // Schedule the first tick immediately. The main loop will
                            // handle publishing it and scheduling the subsequent tick.
                            next_tick_at = Instant::now();
                            clock_publisher.publish(ClockEvent::Start).await;
                            MIDI_CHANNEL
                                .send(LiveEvent::Realtime(SystemRealtime::Start))
                                .await;
                        } else if is_running && !next_is_running {
                            clock_publisher.publish(ClockEvent::Reset).await;
                            MIDI_CHANNEL
                                .send(LiveEvent::Realtime(SystemRealtime::Stop))
                                .await;
                        }
                        is_running = next_is_running;
                    }
                }
            }
        }
    };

    // let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    //
    // let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    // let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    // join4(internal_fut, atom_fut, meteor_fut, cube_fut).await;
    internal_fut.await;
}
