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
use midly::live::SystemRealtime;
use portable_atomic::Ordering;

use libfp::{utils::bpm_to_clock_duration, AuxJackMode, ClockSrc, GlobalConfig};

use crate::{
    tasks::{
        max::MAX_TRIGGERS_GPO,
        midi::{MidiClockTarget, MidiOutEvent, MIDI_CHANNEL},
    },
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

pub static CLOCK_IN_CHANNEL: Channel<ThreadModeRawMutex, ClockInEvent, 16> = Channel::new();
pub static TRANSPORT_CMD_CHANNEL: Channel<ThreadModeRawMutex, TransportCmd, 8> = Channel::new();

#[derive(Clone, Copy)]
pub enum ClockInEvent {
    Tick(ClockSrc),
    Start(ClockSrc),
    Stop(ClockSrc),
    Reset(ClockSrc),
    Continue(ClockSrc),
}

#[derive(Clone, Copy)]
pub enum TransportCmd {
    Start,
    Stop,
    Toggle,
}

#[derive(Clone, Copy)]
pub enum ClockEvent {
    Tick,
    Start,
    Stop,
    Reset,
}

const INTERNAL_PPQN: u8 = 24;

pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock_sources(aux_inputs)).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
}

async fn make_ext_clock_loop(mut pin: Input<'_>, clock_src: ClockSrc) {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let mut current_config = config_receiver.get().await;
    let clock_in_sender = CLOCK_IN_CHANNEL.sender();

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
                    ClockInEvent::Reset(clock_src)
                } else {
                    ClockInEvent::Tick(clock_src)
                };

                clock_in_sender.send(clock_event).await;
            }
            Either::Second(new_config) => {
                // Config change happened.
                current_config = new_config;
            }
        }
    }
}

async fn send_analog_ticks(spawner: &Spawner, config: &GlobalConfig, counters: &mut [u16; 3]) {
    for (i, aux) in config.aux.iter().enumerate() {
        if let AuxJackMode::ClockOut(div) = aux {
            if counters[i] == 0 {
                // TODO: Adjust trigger_len based on division?
                spawner.spawn(analog_tick(i, 5)).unwrap();
            }

            counters[i] += 1;
            if counters[i] >= *div as u16 {
                counters[i] = 0;
            }
        }
    }
}

async fn send_analog_reset(spawner: &Spawner, config: &GlobalConfig) {
    for (i, aux) in config.aux.iter().enumerate() {
        if let AuxJackMode::ResetOut = aux {
            // Send reset pulse with longer duration (10ms)
            spawner.spawn(analog_tick(i, 10)).unwrap();
        }
    }
}
#[embassy_executor::task(pool_size = 6)]
async fn analog_tick(aux_no: usize, trigger_len: u64) {
    let gpo_index = 17 + aux_no;
    MAX_TRIGGERS_GPO[gpo_index].store(2, Ordering::Relaxed);
    Timer::after_millis(trigger_len).await;
    MAX_TRIGGERS_GPO[gpo_index].store(1, Ordering::Relaxed);
}

#[embassy_executor::task]
async fn run_clock_gatekeeper() {
    let clock_publisher = CLOCK_PUBSUB.publisher().unwrap();
    let midi_sender = MIDI_CHANNEL.sender();
    let clock_in_receiver = CLOCK_IN_CHANNEL.receiver();
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();

    let spawner = Spawner::for_current_executor().await;

    let mut config = config_receiver.get().await;
    let mut is_running = false;
    let mut analog_tick_counters: [u16; 3] = [0; 3];

    loop {
        match select(clock_in_receiver.receive(), config_receiver.changed()).await {
            Either::First(event) => {
                let (is_event_for_tick, is_event_for_reset, source) = match event {
                    ClockInEvent::Tick(s)
                    | ClockInEvent::Start(s)
                    | ClockInEvent::Stop(s)
                    | ClockInEvent::Continue(s) => (true, false, s),
                    ClockInEvent::Reset(s) => (false, true, s),
                };

                let is_active_clock_source = is_event_for_tick && source == config.clock.clock_src;
                let is_active_reset_source = is_event_for_reset && source == config.clock.reset_src;

                if !is_active_clock_source && !is_active_reset_source {
                    continue;
                }

                // Determine MIDI routing target
                let midi_target = match source {
                    ClockSrc::MidiUsb => MidiClockTarget::Uart,
                    ClockSrc::MidiIn => MidiClockTarget::Usb,
                    _ => MidiClockTarget::Both,
                };

                // Process the event
                match event {
                    // Clock tick. Only process if clock is running
                    ClockInEvent::Tick(source) => {
                        if is_running {
                            clock_publisher.publish(ClockEvent::Tick).await;
                            send_analog_ticks(&spawner, &config, &mut analog_tick_counters).await;
                            let _ = midi_sender.try_send(MidiOutEvent::Clock(
                                SystemRealtime::TimingClock,
                                midi_target,
                            ));
                        }
                    }
                    // Start the clock without resetting the phase
                    ClockInEvent::Continue(source) => {
                        is_running = true;
                        clock_publisher.publish(ClockEvent::Start).await;
                        let _ = midi_sender
                            .try_send(MidiOutEvent::Clock(SystemRealtime::Continue, midi_target));
                    }
                    // (Re-)start the clock. Full phase reset
                    ClockInEvent::Start(source) => {
                        is_running = true;
                        clock_publisher.publish(ClockEvent::Reset).await;
                        clock_publisher.publish(ClockEvent::Start).await;
                        analog_tick_counters = [0; 3];
                        send_analog_reset(&spawner, &config).await;
                        let _ = midi_sender
                            .try_send(MidiOutEvent::Clock(SystemRealtime::Start, midi_target));
                    }
                    // Stop the clock. No phase reset
                    ClockInEvent::Stop(source) => {
                        is_running = false;
                        clock_publisher.publish(ClockEvent::Stop).await;
                        let _ = midi_sender
                            .try_send(MidiOutEvent::Clock(SystemRealtime::Stop, midi_target));
                    }
                    // Reset the phase without affecting the run state
                    ClockInEvent::Reset(source) => {
                        clock_publisher.publish(ClockEvent::Reset).await;
                        analog_tick_counters = [0; 3];
                        send_analog_reset(&spawner, &config).await;
                        let _ = midi_sender
                            .try_send(MidiOutEvent::Clock(SystemRealtime::Reset, midi_target));
                    }
                }
            }
            Either::Second(new_config) => {
                // If the clock source has been changed, reset the running state.
                if config.clock.clock_src != new_config.clock.clock_src {
                    is_running = false;
                    analog_tick_counters = [0; 3];
                }
                config = new_config;
            }
        }
    }
}

// TODO: Rework the clock to use only one tick generator with various tempo and sync sources
#[embassy_executor::task]
async fn run_clock_sources(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Down);
    let meteor = Input::new(meteor_pin, Pull::Down);
    let cube = Input::new(hexagon_pin, Pull::Down);

    let internal_fut = async {
        let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
        let clock_in_sender = CLOCK_IN_CHANNEL.sender();
        let clock_receiver = TRANSPORT_CMD_CHANNEL.receiver();

        let config = config_receiver.get().await;
        let mut tick_duration = bpm_to_clock_duration(config.clock.internal_bpm, INTERNAL_PPQN);
        let mut next_tick_at = Instant::now();
        let mut is_running = false;

        loop {
            // This future only resolves on a tick when the internal clock is active.
            // Otherwise, it pends forever, preventing ticks from being generated.
            let tick_fut = async {
                if is_running {
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
                    clock_in_sender
                        .send(ClockInEvent::Tick(ClockSrc::Internal))
                        .await;
                }
                Either3::Second(new_config) => {
                    let old_tick_duration = tick_duration;
                    let new_tick_duration =
                        bpm_to_clock_duration(new_config.clock.internal_bpm, INTERNAL_PPQN);

                    // If BPM changes while the clock is running, adjust the next tick time
                    // to make the transition smooth.
                    if old_tick_duration != new_tick_duration && is_running {
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

                    tick_duration = new_tick_duration;
                }
                Either3::Third(cmd) => {
                    let next_is_running = match cmd {
                        TransportCmd::Start => true,
                        TransportCmd::Stop => false,
                        TransportCmd::Toggle => !is_running,
                    };

                    if !is_running && next_is_running {
                        // Schedule the first tick immediately. The main loop will
                        // handle publishing it and scheduling the subsequent tick.
                        next_tick_at = Instant::now();
                        clock_in_sender
                            .send(ClockInEvent::Start(ClockSrc::Internal))
                            .await;
                    } else if is_running && !next_is_running {
                        clock_in_sender
                            .send(ClockInEvent::Stop(ClockSrc::Internal))
                            .await;
                    }
                    is_running = next_is_running;
                }
            }
        }
    };

    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    join4(internal_fut, atom_fut, meteor_fut, cube_fut).await;
}
