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
use heapless::Deque;
use midly::live::SystemRealtime;
use portable_atomic::{AtomicBool, AtomicU64, Ordering};

use libfp::{
    utils::bpm_to_clock_duration, AuxJackMode, ClockSrc, GlobalConfig, MidiOut, MidiOutConfig,
};

use max11300::config::Port;

use crate::{
    state::{is_clock_running, update_state},
    tasks::{
        max::{MaxCmd, MAX_CHANNEL},
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
    MidiTick(ClockSrc),
    Start(ClockSrc),
    Stop(ClockSrc),
    Reset(ClockSrc),
    Continue(ClockSrc),
}

impl ClockInEvent {
    pub fn source(&self) -> ClockSrc {
        match self {
            Self::Tick(s)
            | Self::MidiTick(s)
            | Self::Start(s)
            | Self::Stop(s)
            | Self::Reset(s)
            | Self::Continue(s) => *s,
        }
    }
    pub fn is_clock(&self) -> bool {
        matches!(self, Self::Tick(_) | Self::MidiTick(_))
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

/// Half of the swing window, in 24-PPQN ticks. With `H = 6`, the swing window
/// is one 8th note (12 ticks) and swing is applied at the 16th-note level.
const SWING_HALF_INTERVAL: u32 = 6;

/// Capacity of the external-clock pending-emission queue. One swing window of
/// 2H = 12 ticks is scheduled up-front on the window-start pulse; capacity 32
/// leaves ample headroom for transient jitter.
const PENDING_EMISSIONS_CAPACITY: usize = 32;

/// Swung absolute offset of tick `i` (in `[0, 2H]`) from the start of the swing
/// window. Used by both the internal clock (to schedule the next tick directly)
/// and the external clock (to schedule the whole window on its anchor pulse).
///
/// The result is clamped to 500µs before the window boundary. Without this,
/// heavy positive swing pushes the last ticks of the window past the boundary,
/// causing the engine to fire tick 0 of the next window as an immediate
/// catch-up. That catch-up creates two ticks in rapid succession: the
/// gatekeeper processes both before any subscriber runs, incrementing
/// TICK_COUNTER twice, so subscribers read the same stale counter for both
/// events and double-fire notes on beat boundaries.
fn swung_offset(i: u32, t: Duration, swing: i8) -> Duration {
    let h = SWING_HALF_INTERVAL as i64;
    let t_ticks = t.as_ticks() as i64;
    let s = swing as i64;
    let i = i as i64;

    let raw = if i < h {
        // First 16th note: normal spacing
        i * t_ticks
    } else {
        // Second 16th note: shifted start, normal spacing within
        let boundary = h * t_ticks * (50 + s) / 50;
        boundary + (i - h) * t_ticks
    };

    // Clamp to 500µs before the window end. A 1µs margin was insufficient:
    // by the time the next loop iteration polls `Timer::at(next_tick_at)`,
    // several µs of code execution have elapsed, consuming the gap and causing
    // the timer to fire immediately — no executor yield, same race. 500µs is
    // larger than any plausible Arm 4 round-trip.
    let window_end = 2 * h * t_ticks - 500;
    Duration::from_ticks((raw.max(0) as u64).min(window_end as u64))
}

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
    let mut ports: heapless::Vec<Port, 4> = heapless::Vec::new();
    for (i, aux) in config.aux.iter().enumerate() {
        if let AuxJackMode::ClockOut(div) = aux {
            if counters[i] == 0 {
                let _ = ports.push(Port::try_from(17 + i).unwrap());
            }

            counters[i] += 1;
            if counters[i] >= *div as u16 {
                counters[i] = 0;
            }
        }
    }
    if !ports.is_empty() {
        MAX_CHANNEL
            .sender()
            .send(MaxCmd::GpoSetHighMany(ports.clone()))
            .await;
        spawner.spawn(analog_tick_release(ports, 5)).ok();
    }
}

async fn send_analog_reset(spawner: &Spawner, config: &GlobalConfig) {
    let mut ports: heapless::Vec<Port, 4> = heapless::Vec::new();
    for (i, aux) in config.aux.iter().enumerate() {
        if let AuxJackMode::ResetOut = aux {
            let _ = ports.push(Port::try_from(17 + i).unwrap());
        }
    }
    if !ports.is_empty() {
        MAX_CHANNEL
            .sender()
            .send(MaxCmd::GpoSetHighMany(ports.clone()))
            .await;
        spawner.spawn(analog_tick_release(ports, 10)).ok();
    }
}

#[embassy_executor::task(pool_size = 4)]
async fn analog_tick_release(ports: heapless::Vec<Port, 4>, trigger_len: u64) {
    Timer::after_millis(trigger_len).await;
    MAX_CHANNEL
        .sender()
        .send(MaxCmd::GpoSetLowMany(ports))
        .await;
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
                    | ClockInEvent::MidiTick(s)
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
                        }
                    }
                    // Unswung MIDI clock tick — forwarded to MIDI outputs at the straight rate
                    ClockInEvent::MidiTick(source) => {
                        if is_running
                            || matches!(source, ClockSrc::Atom | ClockSrc::Meteor | ClockSrc::Cube)
                        {
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
    // `window_start_at` is the (unswung) time of tick 0 of the current swing
    // window. For internal, it replaces the old per-tick `last_tick_at` — the
    // swung schedule is computed relative to this anchor.
    let startup_anchor = Instant::now() + Duration::from_millis(TICK_RESET_DELAY as u64);
    let mut window_start_at = startup_anchor;
    let mut tick_in_window: u32 = 0;
    let mut next_tick_at = startup_anchor;
    let mut next_midi_tick_at = startup_anchor;
    let mut last_pulse: Option<Instant> = None;
    // Measured period between external pulses. `None` until we have enough pulses
    // to compute a rolling average, which gates the external watchdog so it can't
    // fire based on a stale internal-BPM-derived duration.
    let mut measured_ext_period: Option<Duration> = None;
    let mut delta_history: [Duration; HISTORY_SIZE] = [Duration::from_ticks(0); HISTORY_SIZE];
    let mut history_idx: usize = 0;
    // Queued swung emissions for the external clock path. Each entry is the
    // absolute emission time for the front-most unpublished tick. Empty in the
    // internal or straight-passthrough (`swing == 0`) case.
    let mut pending_emissions: Deque<Instant, PENDING_EMISSIONS_CAPACITY> = Deque::new();
    // True if the current swing window's ticks were pre-scheduled on its
    // anchor pulse. Mid-window pulses check this flag to decide whether
    // to suppress (predicted — emission already queued) or fall through
    // to straight passthrough (not predicted — e.g. swing was 0 or there
    // was no measured period at the anchor). Cleared at window rollover.
    let mut window_predicted = false;
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
        // - Internal + running: fire on swung tick schedule (`next_tick_at`)
        // - External + running: earliest of watchdog (`next_tick_at`) and the
        //   front of the pending-emission queue
        // - Otherwise: pend forever
        let timer_fut = async {
            if !is_running {
                core::future::pending::<()>().await;
                return;
            }
            if config.clock.clock_src == ClockSrc::Internal {
                Timer::at(next_tick_at.min(next_midi_tick_at)).await;
            } else if last_pulse.is_some() && measured_ext_period.is_some() {
                // Watchdog is armed; also consider any pending swung emission
                // that may be due sooner.
                let deadline = pending_emissions
                    .front()
                    .copied()
                    .map(|e| e.min(next_tick_at))
                    .unwrap_or(next_tick_at);
                Timer::at(deadline).await;
            } else if let Some(&front) = pending_emissions.front() {
                // Measurement lost but queue still has emissions (edge case).
                Timer::at(front).await;
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
                    pending_emissions.clear();
                    tick_in_window = 0;

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
                        window_start_at = Instant::now();
                        next_midi_tick_at = window_start_at;
                    }
                } else if config.clock.clock_src == ClockSrc::Internal {
                    // BPM or swing change while on internal source.
                    let new_tick_duration =
                        bpm_to_clock_duration(new_config.clock.internal_bpm, INTERNAL_PPQN);
                    let bpm_changed = current_tick_duration != new_tick_duration;
                    let swing_changed = config.clock.swing_amount != new_config.clock.swing_amount;

                    current_tick_duration = new_tick_duration;

                    if is_running && (bpm_changed || swing_changed) {
                        // Recompute the next tick from the fixed window anchor.
                        // Keeping `window_start_at` put preserves the grid and
                        // the swing shape across live nudges.
                        next_tick_at = window_start_at
                            + swung_offset(
                                tick_in_window,
                                current_tick_duration,
                                new_config.clock.swing_amount,
                            );
                    }
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
                        window_start_at =
                            Instant::now() + Duration::from_millis(TICK_RESET_DELAY as u64);
                        tick_in_window = 0;
                        next_tick_at = window_start_at;
                        next_midi_tick_at = window_start_at;
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
                        ClockInEvent::Start(_) => {
                            is_running = true;
                            // Fresh downbeat: drop any stale swung emissions and
                            // re-anchor the window on the next pulse.
                            pending_emissions.clear();
                            tick_in_window = 0;
                        }
                        ClockInEvent::Continue(_) => {
                            is_running = true;
                        }
                        ClockInEvent::Stop(_) => {
                            is_running = false;
                            pending_emissions.clear();
                        }
                        ClockInEvent::Reset(_) => {
                            pending_emissions.clear();
                            tick_in_window = 0;
                        }
                        _ => {}
                    }
                }
                SyncEngineEvent::Pulse { source, timestamp } => {
                    // Check if this pulse is from the reset source
                    let reset_src: ClockSrc = config.clock.reset_src.into();
                    if source == reset_src && reset_src != ClockSrc::None {
                        clock_in_sender.send(ClockInEvent::Reset(source)).await;
                        pending_emissions.clear();
                        tick_in_window = 0;
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

                    // Window-relative scheduling on external:
                    //
                    // - At `tick_in_window == 0` (window anchor), anchor the
                    //   window to this pulse and, if we have a measured period
                    //   and non-zero swing, pre-schedule all `2H` emissions
                    //   for the window using `swung_offset`. This lets
                    //   negative swing emit *earlier* than the unswung grid
                    //   without any latency buffer, because we know where
                    //   every tick in the window will land the moment we
                    //   anchor it.
                    // - Mid-window pulses are consumed for measurement and
                    //   watchdog only; their emissions were already queued at
                    //   window start.
                    // - On `S = 0` or before the period has been measured,
                    //   fall back to straight passthrough — forward every
                    //   pulse immediately, no queue. This also covers the
                    //   first window after Start / Reset / source change,
                    //   which has no prior period to base a prediction on.

                    // Forward every raw pulse as an unswung MIDI clock tick.
                    clock_in_sender.send(ClockInEvent::MidiTick(source)).await;

                    let swing = config.clock.swing_amount;
                    if tick_in_window == 0 {
                        // Window anchor: decide the mode for this whole
                        // window based on the state *right now*, and stick
                        // with it until the next anchor.
                        window_start_at = timestamp;
                        match measured_ext_period {
                            Some(t) if swing != 0 => {
                                // Pre-schedule all 2H emissions for the window.
                                for i in 0..(2 * SWING_HALF_INTERVAL) {
                                    let emission = window_start_at + swung_offset(i, t, swing);
                                    // Belt-and-braces monotonicity guard
                                    // against any stale entries still sitting
                                    // in the queue from a prior window that
                                    // straddled a tempo transition.
                                    let clamped = match pending_emissions.back() {
                                        Some(&back) if emission < back => back,
                                        _ => emission,
                                    };
                                    if pending_emissions.is_full() {
                                        pending_emissions.pop_front();
                                    }
                                    let _ = pending_emissions.push_back(clamped);
                                }
                                window_predicted = true;
                            }
                            _ => {
                                // No prediction — straight passthrough for
                                // the anchor pulse and the rest of this
                                // window, even if swing or the measured
                                // period change mid-window. The next window
                                // picks the mode fresh.
                                clock_in_sender.send(ClockInEvent::Tick(source)).await;
                                window_predicted = false;
                            }
                        }
                    } else if !window_predicted {
                        // Mid-window pulse under an unpredicted window —
                        // forward it straight.
                        clock_in_sender.send(ClockInEvent::Tick(source)).await;
                    }
                    // else: mid-window pulse under an active prediction —
                    // emission is already queued, nothing to do here.

                    tick_in_window += 1;
                    if tick_in_window >= 2 * SWING_HALF_INTERVAL {
                        tick_in_window = 0;
                    }

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
                    let now = Instant::now();
                    // Unswung MIDI clock: fires at the nominal (straight) cadence
                    if now >= next_midi_tick_at {
                        clock_in_sender
                            .send(ClockInEvent::MidiTick(ClockSrc::Internal))
                            .await;
                        next_midi_tick_at += current_tick_duration;
                    }
                    // Swung internal tick: fires at the swing-adjusted time
                    if now >= next_tick_at {
                        clock_in_sender
                            .send(ClockInEvent::Tick(ClockSrc::Internal))
                            .await;
                        tick_in_window += 1;
                        if tick_in_window >= 2 * SWING_HALF_INTERVAL {
                            tick_in_window = 0;
                            window_start_at += current_tick_duration * (2 * SWING_HALF_INTERVAL);
                        }
                        next_tick_at = window_start_at
                            + swung_offset(
                                tick_in_window,
                                current_tick_duration,
                                config.clock.swing_amount,
                            );
                    }
                } else if is_running {
                    // External: either a pending swung emission is due, or the
                    // watchdog fired (external clock lost).
                    let now = Instant::now();
                    let popped = if let Some(&front) = pending_emissions.front() {
                        if front <= now {
                            pending_emissions.pop_front();
                            clock_in_sender
                                .send(ClockInEvent::Tick(config.clock.clock_src))
                                .await;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !popped && last_pulse.is_some() && now >= next_tick_at {
                        // Watchdog: external clock lost
                        clock_in_sender
                            .send(ClockInEvent::Stop(config.clock.clock_src))
                            .await;
                        last_pulse = None;
                        is_running = false;
                        pending_emissions.clear();
                        tick_in_window = 0;
                        spawner.spawn(store_clock_running(false)).ok();
                    }
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
