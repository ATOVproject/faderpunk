use embassy_futures::{
    join::join4,
    select::{select, select4, Either, Either4},
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
use embassy_time::{Duration, Instant, Timer};
use midly::live::SystemRealtime;
use portable_atomic::{AtomicBool, AtomicU64, Ordering};

use libfp::{
    utils::bpm_to_clock_duration, AuxJackMode, ClockSrc, GlobalConfig, MidiOut, MidiOutConfig,
};

use crate::{
    state::{is_clock_running, update_state},
    tasks::{
        max::MAX_TRIGGERS_GPO,
        midi::{MidiClockMsg, MidiOutEvent, MIDI_CHANNEL},
    },
    Spawner, GLOBAL_CONFIG_WATCH,
};

const CLOCK_PUBSUB_SIZE: usize = 16;
// 16 apps + 1 metronome
const CLOCK_PUBSUB_SUBSCRIBERS: usize = 17;
// Only the gatekeeper publishes to CLOCK_PUBSUB
const CLOCK_PUBSUB_PUBLISHERS: usize = 5;
// Add a slight delay before the very first tick (to offset it to reset)
const TICK_RESET_DELAY: u8 = 2;
/// PPQN of the internal clock
const INTERNAL_PPQN: u8 = 24;
/// How long METRONOME_HIGH stays true after each beat (ms).
const METRONOME_HIGH_MS: u64 = 25;

pub static TICK_COUNTER: AtomicU64 = AtomicU64::new(0);
pub static METRONOME_HIGH: AtomicBool = AtomicBool::new(true);

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

impl ClockInEvent {
    pub fn source(&self) -> ClockSrc {
        match self {
            Self::Tick(s) | Self::Start(s) | Self::Stop(s) | Self::Reset(s) | Self::Continue(s) => {
                *s
            }
        }
    }
    pub fn is_clock(&self) -> bool {
        if let Self::Tick(_) = self {
            return true;
        }
        false
    }
    #[allow(dead_code)]
    pub fn is_transport(&self) -> bool {
        !self.is_clock()
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum TransportCmd {
    Start,
    Stop,
    Toggle,
}

/// Events emitted by the clock task and received via [`Clock::wait_for_event`].
#[derive(Clone, Copy)]
pub enum ClockEvent {
    /// Clock pulse triggering at the set PPQN division.
    /// Tick counter reports the number of 24ppqn ticks since the last reset.
    Tick,
    /// The clock has started or resumed playback (no phase reset).
    Start,
    /// The clock has stopped. No phase reset; notes/gates should be silenced.
    Stop,
    /// A full phase reset. The next tick counter value will be `0`.
    Reset,
}

#[derive(Clone, Copy)]
pub enum SyncEngineEvent {
    /// A timing pulse from an analog pin or MIDI TimingClock
    Pulse {
        source: ClockSrc,
        timestamp: Instant,
    },
    /// A transport command from an external source (MIDI Start/Stop/Continue/Reset)
    Transport(ClockInEvent),
}

pub static SYNC_ENGINE_CHANNEL: Channel<ThreadModeRawMutex, SyncEngineEvent, 16> = Channel::new();

/// Debounce window for analog clock pins. Long enough to swallow any plausible
/// edge bounce, short enough to never clip a real pulse from a fast Eurorack clock
/// (24 PPQN at 1200 BPM ≈ 2.1ms; 48 PPQN at 600 BPM ≈ 2.1ms).
const DEBOUNCE_THRESHOLD: Duration = Duration::from_millis(2);

/// Rolling-average window for measured pulse interval. Larger = smoother but
/// slower to widen the watchdog when tempo ramps down.
const HISTORY_SIZE: usize = 4;

/// Maximum tempo slowdown (in measured-period multiples) we'll tolerate between
/// two consecutive pulses before declaring the external clock lost. 8× covers
/// any musical tempo change short of an actual stop.
const WATCHDOG_MULTIPLIER: u32 = 8;

/// Absolute upper bound on the gap between two pulses, regardless of measured
/// rate. Sized for the slowest plausible Eurorack source (≈1 PPQN at 30 BPM).
/// Raise this to support slower clocks; lower it for faster Stop detection.
const WATCHDOG_FLOOR: Duration = Duration::from_millis(2000);

pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock_sources(aux_inputs)).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
    spawner.spawn(metronome()).unwrap();
}

async fn make_ext_clock_loop(mut pin: Input<'_>, clock_src: ClockSrc) {
    let sender = SYNC_ENGINE_CHANNEL.sender();
    loop {
        pin.wait_for_falling_edge().await;
        pin.wait_for_low().await;
        sender
            .send(SyncEngineEvent::Pulse {
                source: clock_src,
                timestamp: Instant::now(),
            })
            .await;
    }
}

async fn send_analog_ticks(spawner: &Spawner, config: &GlobalConfig, counters: &mut [u16; 3]) {
    for (i, aux) in config.aux.iter().enumerate() {
        if let AuxJackMode::ClockOut(div) = aux {
            if counters[i] == 0 {
                // TODO: Adjust trigger_len based on division?
                // Ignore if task pool is full - skip this tick rather than panic
                spawner.spawn(analog_tick(i, 5)).ok();
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
            // Ignore if task pool is full - skip this reset rather than panic
            spawner.spawn(analog_tick(i, 10)).ok();
        }
    }
}
#[embassy_executor::task(pool_size = 12)]
async fn analog_tick(aux_no: usize, trigger_len: u64) {
    let gpo_index = 17 + aux_no;
    MAX_TRIGGERS_GPO[gpo_index].store(2, Ordering::Relaxed);
    Timer::after_millis(trigger_len).await;
    MAX_TRIGGERS_GPO[gpo_index].store(1, Ordering::Relaxed);
}

#[embassy_executor::task]
async fn metronome() {
    let mut sub = CLOCK_PUBSUB.subscriber().unwrap();
    let mut tick_count: u64 = 0;

    loop {
        match sub.next_message_pure().await {
            ClockEvent::Tick => {
                tick_count += 1;
                // Fire on the first tick of each quarter note (every 24 ppqn ticks).
                if tick_count % 24 == 1 {
                    METRONOME_HIGH.store(true, Ordering::Relaxed);
                    Timer::after_millis(METRONOME_HIGH_MS).await;
                    METRONOME_HIGH.store(false, Ordering::Relaxed);
                }
            }
            ClockEvent::Start | ClockEvent::Reset => {
                tick_count = 0;
                METRONOME_HIGH.store(true, Ordering::Relaxed);
            }
            ClockEvent::Stop => {
                METRONOME_HIGH.store(false, Ordering::Relaxed);
            }
        }
    }
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
                let (is_active, _source) = match event {
                    ClockInEvent::Tick(s)
                    | ClockInEvent::Start(s)
                    | ClockInEvent::Stop(s)
                    | ClockInEvent::Continue(s) => (s == config.clock.clock_src, s),
                    ClockInEvent::Reset(s) => (s == config.clock.reset_src.into(), s),
                };

                if !is_active {
                    continue;
                }

                // Determine MIDI routing target
                let midi_targets = if event.is_clock() {
                    config.midi.outs.map(|c| {
                        matches!(
                            c,
                            MidiOutConfig {
                                send_clock: true,
                                ..
                            }
                        )
                    })
                } else {
                    config.midi.outs.map(|c| {
                        matches!(
                            c,
                            MidiOutConfig {
                                send_transport: true,
                                ..
                            },
                        )
                    })
                };

                let midi_target = MidiOut(midi_targets);
                let should_send_midi = midi_targets.iter().any(|&t| t);

                // Process the event
                let mut midi_rt_event: Option<SystemRealtime> = None;
                match event {
                    // Clock tick. Only process if clock is running
                    ClockInEvent::Tick(source) => {
                        if is_running
                            || matches!(source, ClockSrc::Atom | ClockSrc::Meteor | ClockSrc::Cube)
                        {
                            // Relies on AtomicU64 wrapping on overflow MAX + 1 to ensure first reported TICK_COUNTER after a Clock::Start is always 0
                            TICK_COUNTER.fetch_add(1, Ordering::Relaxed);
                            clock_publisher.publish(ClockEvent::Tick).await;
                            send_analog_ticks(&spawner, &config, &mut analog_tick_counters).await;
                            midi_rt_event = Some(SystemRealtime::TimingClock);
                        }
                    }
                    // Start the clock without resetting the phase
                    ClockInEvent::Continue(_) => {
                        is_running = true;
                        clock_publisher.publish(ClockEvent::Start).await;
                        midi_rt_event = Some(SystemRealtime::Continue);
                    }
                    // (Re-)start the clock. Full phase reset
                    ClockInEvent::Start(_) => {
                        TICK_COUNTER.store(u64::MAX, Ordering::Relaxed);
                        is_running = true;
                        clock_publisher.publish(ClockEvent::Reset).await;
                        clock_publisher.publish(ClockEvent::Start).await;
                        analog_tick_counters = [0; 3];
                        send_analog_reset(&spawner, &config).await;
                        midi_rt_event = Some(SystemRealtime::Start);
                    }
                    // Stop the clock. No phase reset
                    ClockInEvent::Stop(_) => {
                        is_running = false;
                        clock_publisher.publish(ClockEvent::Stop).await;
                        midi_rt_event = Some(SystemRealtime::Stop);
                    }
                    // Reset the phase without affecting the run state
                    ClockInEvent::Reset(_) => {
                        TICK_COUNTER.store(u64::MAX, Ordering::Relaxed);
                        clock_publisher.publish(ClockEvent::Reset).await;
                        analog_tick_counters = [0; 3];
                        send_analog_reset(&spawner, &config).await;
                        midi_rt_event = Some(SystemRealtime::Reset);
                    }
                }

                if should_send_midi {
                    if let Some(rt_event) = midi_rt_event {
                        let msg = MidiClockMsg::new(rt_event, midi_target);
                        let _ = midi_sender.try_send(MidiOutEvent::Clock(msg));
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

#[embassy_executor::task]
async fn store_clock_running(is_running: bool) {
    update_state(|s| {
        s.clock_is_running = is_running;
        // We're already checking below whether the clock status changed
        true
    })
    .await;
}

async fn run_unified_clock_engine() {
    let mut config_receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let clock_in_sender = CLOCK_IN_CHANNEL.sender();
    let sync_engine_receiver = SYNC_ENGINE_CHANNEL.receiver();
    let transport_receiver = TRANSPORT_CMD_CHANNEL.receiver();
    let spawner = Spawner::for_current_executor().await;

    let config = config_receiver.get().await;
    let mut is_running = is_clock_running().await;
    let mut current_tick_duration = bpm_to_clock_duration(config.clock.internal_bpm, INTERNAL_PPQN);
    let mut last_tick_at = Instant::now();
    let mut next_tick_at = Instant::now() + Duration::from_millis(TICK_RESET_DELAY as u64);
    let mut last_pulse: Option<Instant> = None;
    // Measured period between external pulses. `None` until we have enough pulses
    // to compute a rolling average, which gates the external watchdog so it can't
    // fire based on a stale internal-BPM-derived duration.
    let mut measured_ext_period: Option<Duration> = None;
    let mut delta_history: [Duration; HISTORY_SIZE] = [Duration::from_ticks(0); HISTORY_SIZE];
    let mut history_idx: usize = 0;
    let mut config = config;

    // If clock was already running at startup (persisted state) with internal source,
    // inform the gatekeeper to synchronize its state.
    if is_running && config.clock.clock_src == ClockSrc::Internal {
        clock_in_sender
            .send(ClockInEvent::Start(ClockSrc::Internal))
            .await;
    }

    loop {
        // Timer future depends on mode:
        // - Internal + running: fire on tick schedule
        // - External + running + has pulse: watchdog timer
        // - Otherwise: pend forever
        let timer_fut = async {
            let should_tick = if config.clock.clock_src == ClockSrc::Internal {
                is_running
            } else {
                // For external sources, only arm the watchdog once we've actually
                // measured the inter-pulse period. Otherwise we'd kill the clock
                // based on the stale internal-BPM-derived `current_tick_duration`.
                is_running && last_pulse.is_some() && measured_ext_period.is_some()
            };
            if should_tick {
                Timer::at(next_tick_at).await;
            } else {
                core::future::pending::<()>().await;
            }
        };

        match select4(
            config_receiver.changed(),
            transport_receiver.receive(),
            sync_engine_receiver.receive(),
            timer_fut,
        )
        .await
        {
            // Arm 1: Config changes
            Either4::First(new_config) => {
                if config.clock.clock_src != new_config.clock.clock_src {
                    // Source changed: reset external tracking state
                    last_pulse = None;
                    measured_ext_period = None;
                    delta_history = [Duration::from_ticks(0); HISTORY_SIZE];
                    history_idx = 0;

                    // Drop transport state to match the gatekeeper's behavior,
                    // which also resets is_running on source change.
                    if is_running {
                        is_running = false;
                        spawner.spawn(store_clock_running(false)).ok();
                    }

                    if new_config.clock.clock_src == ClockSrc::Internal {
                        // Switching to internal: recalculate tick duration from BPM
                        current_tick_duration =
                            bpm_to_clock_duration(new_config.clock.internal_bpm, INTERNAL_PPQN);
                        last_tick_at = Instant::now();
                    }
                } else if config.clock.clock_src == ClockSrc::Internal {
                    // BPM change while on internal source
                    let old_tick_duration = current_tick_duration;
                    let new_tick_duration =
                        bpm_to_clock_duration(new_config.clock.internal_bpm, INTERNAL_PPQN);

                    // Recompute next tick from the grid anchor to avoid drift
                    if old_tick_duration != new_tick_duration && is_running {
                        next_tick_at = last_tick_at + new_tick_duration;
                    }

                    current_tick_duration = new_tick_duration;
                }

                config = new_config;
            }

            // Arm 2: Transport commands from UI (only effective for internal clock)
            Either4::Second(cmd) => {
                if config.clock.clock_src != ClockSrc::Internal {
                    continue;
                }

                let next_is_running = match cmd {
                    TransportCmd::Start => true,
                    TransportCmd::Stop => false,
                    TransportCmd::Toggle => !is_running,
                };

                if is_running != next_is_running {
                    if next_is_running {
                        last_tick_at = Instant::now();
                        next_tick_at =
                            last_tick_at + Duration::from_millis(TICK_RESET_DELAY as u64);
                        clock_in_sender
                            .send(ClockInEvent::Start(ClockSrc::Internal))
                            .await;
                    } else {
                        clock_in_sender
                            .send(ClockInEvent::Stop(ClockSrc::Internal))
                            .await;
                    }
                    is_running = next_is_running;
                    spawner.spawn(store_clock_running(is_running)).ok();
                }
            }

            // Arm 3: Sync engine events (from ext pins and MIDI)
            Either4::Third(sync_event) => match sync_event {
                SyncEngineEvent::Transport(event) => {
                    // Only forward transport events that match the active clock source
                    if event.source() != config.clock.clock_src {
                        continue;
                    }
                    clock_in_sender.send(event).await;
                    match event {
                        ClockInEvent::Start(_) | ClockInEvent::Continue(_) => {
                            is_running = true;
                        }
                        ClockInEvent::Stop(_) => {
                            is_running = false;
                        }
                        _ => {}
                    }
                }
                SyncEngineEvent::Pulse { source, timestamp } => {
                    // Check if this pulse is from the reset source
                    let reset_src: ClockSrc = config.clock.reset_src.into();
                    if source == reset_src && reset_src != ClockSrc::None {
                        clock_in_sender.send(ClockInEvent::Reset(source)).await;
                        continue;
                    }

                    // Only process pulses from the active clock source
                    if source != config.clock.clock_src {
                        continue;
                    }

                    // Debounce: discard pulses that arrive too quickly. Only applies
                    // to analog clock-in pins (which can bounce on a switching edge);
                    // MIDI clock is already digital and arrives in bursty USB packets,
                    // so debouncing it would silently drop legitimate ticks at high BPM.
                    let is_analog =
                        matches!(source, ClockSrc::Atom | ClockSrc::Meteor | ClockSrc::Cube);
                    if is_analog {
                        if let Some(last) = last_pulse {
                            if timestamp.duration_since(last) < DEBOUNCE_THRESHOLD {
                                continue;
                            }
                        }
                    }

                    // Phase snap: forward tick immediately
                    clock_in_sender.send(ClockInEvent::Tick(source)).await;

                    // Frequency tracking: compute rolling average of pulse intervals
                    if let Some(last) = last_pulse {
                        let delta = timestamp.duration_since(last);
                        delta_history[history_idx] = delta;
                        history_idx = (history_idx + 1) % HISTORY_SIZE;

                        let mut sum: u64 = 0;
                        let mut count: u32 = 0;
                        for d in &delta_history {
                            if d.as_ticks() > 0 {
                                sum += d.as_ticks();
                                count += 1;
                            }
                        }
                        if count > 0 {
                            let avg = Duration::from_ticks(sum / count as u64);
                            current_tick_duration = avg;
                            measured_ext_period = Some(avg);
                        }
                    }

                    last_pulse = Some(timestamp);
                    // Schedule watchdog: if no pulse arrives within the watchdog window,
                    // declare external clock lost. Use a generous floor so drastic tempo
                    // changes (or slow analog clocks) don't trip it.
                    let watchdog = measured_ext_period
                        .map(|p| p * WATCHDOG_MULTIPLIER)
                        .unwrap_or(WATCHDOG_FLOOR)
                        .max(WATCHDOG_FLOOR);
                    next_tick_at = timestamp + watchdog;
                }
            },

            // Arm 4: Timer fired
            Either4::Fourth(_) => {
                if config.clock.clock_src == ClockSrc::Internal && is_running {
                    // Internal tick: advance grid anchor and schedule next
                    last_tick_at = next_tick_at;
                    next_tick_at = last_tick_at + current_tick_duration;
                    clock_in_sender
                        .send(ClockInEvent::Tick(ClockSrc::Internal))
                        .await;
                } else if is_running && last_pulse.is_some() {
                    // Watchdog: external clock lost
                    clock_in_sender
                        .send(ClockInEvent::Stop(config.clock.clock_src))
                        .await;
                    last_pulse = None;
                    is_running = false;
                    spawner.spawn(store_clock_running(false)).ok();
                }
            }
        }
    }
}

#[embassy_executor::task]
async fn run_clock_sources(aux_inputs: AuxInputs) {
    let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
    let atom = Input::new(atom_pin, Pull::Up);
    let meteor = Input::new(meteor_pin, Pull::Up);
    let cube = Input::new(hexagon_pin, Pull::Up);

    let engine_fut = run_unified_clock_engine();
    let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
    let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
    let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

    join4(engine_fut, atom_fut, meteor_fut, cube_fut).await;
}
