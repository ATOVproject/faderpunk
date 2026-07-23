use embassy_futures::{
    join::{join, join5},
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Instant;
use heapless::Vec;
use midly::{num::u7, MidiMessage};
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{
        bits_7_16, midi_gate, resolution_for_mode, scale_bits_7_12, split_unsigned_value,
        value_to_resolution,
    },
    AppIcon, Brightness, ClockDivision, Color, Config, MidiCc, MidiChannel, MidiIn, MidiNote,
    MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 15;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const MAX_REPEATS: u8 = 8;
const VELOCITY_FLOOR: u16 = 300;
const QUEUE_CAP: usize = 32;
const SOUNDING_CAP: usize = 32;
const GATE_THRESH: u16 = 406;
/// Peak white button flash on note/gate in (full scale).
const INPUT_FLASH_PEAK: u8 = 255;
/// While muted, show the same flash at ~20% so MIDI In stays visible without looking “live”.
const INPUT_FLASH_MUTED_SCALE: u16 = 51; // 51/255 ≈ 20%
/// Straight note-length table for clock-synced delay (same as clk_div "Straight").
const CLOCK_DIVISION_MODE: usize = 0;

/// I/O routing mode.
const IO_MIDI_MIDI: usize = 0;
const IO_MIDI_CV: usize = 1;
const IO_CV_MIDI: usize = 2;

/// Signal role (interpreted in context of I/O mode).
const SIG_PITCH: usize = 0;
const SIG_GATE: usize = 1;
const SIG_CV_CC: usize = 2;
const SIG_GATE_NOTE: usize = 3;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Echolot",
    "MIDI/CV delay with feedback and pitch shift",
    Color::Cyan,
    AppIcon::Sine,
)
.add_param(Param::Enum {
    name: "I/O",
    variants: &["MIDI→MIDI", "MIDI→CV", "CV→MIDI"],
})
.add_param(Param::Enum {
    name: "Delay mode",
    variants: &["ms", "Clock"],
})
.add_param(Param::i32 {
    name: "Max delay (ms)",
    min: 10,
    max: 2000,
})
.add_param(Param::Enum {
    name: "Interval mode",
    variants: &["Fixed", "Stack", "Pong"],
})
.add_param(Param::Enum {
    name: "Routing",
    variants: &["Single", "Ping-Pong"],
})
.add_param(Param::Enum {
    name: "Signal",
    variants: &["Pitch", "Gate", "CV→CC", "Gate→Note"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::MidiIn)
.add_param(Param::MidiChannel {
    name: "MIDI In CH",
})
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel {
    name: "MIDI Out A",
})
.add_param(Param::MidiChannel {
    name: "MIDI Out B",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiNote { name: "MIDI Note" });

pub struct Params {
    io_mode: usize,
    delay_mode: usize,
    max_delay_ms: i32,
    interval_mode: usize,
    routing: usize,
    signal: usize,
    range: Range,
    color: Color,
    midi_in: MidiIn,
    midi_in_ch: MidiChannel,
    midi_out: MidiOut,
    midi_out_a: MidiChannel,
    midi_out_b: MidiChannel,
    midi_cc: MidiCc,
    midi_note: MidiNote,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            io_mode: usize::from_value(values[0]),
            delay_mode: usize::from_value(values[1]),
            max_delay_ms: i32::from_value(values[2]),
            interval_mode: usize::from_value(values[3]),
            routing: usize::from_value(values[4]),
            signal: usize::from_value(values[5]),
            range: Range::from_value(values[6]),
            color: Color::from_value(values[7]),
            midi_in: MidiIn::from_value(values[8]),
            midi_in_ch: MidiChannel::from_value(values[9]),
            midi_out: MidiOut::from_value(values[10]),
            midi_out_a: MidiChannel::from_value(values[11]),
            midi_out_b: MidiChannel::from_value(values[12]),
            midi_cc: MidiCc::from_value(values[13]),
            midi_note: MidiNote::from_value(values[14]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.io_mode.into()).unwrap();
        vec.push(self.delay_mode.into()).unwrap();
        vec.push(self.max_delay_ms.into()).unwrap();
        vec.push(self.interval_mode.into()).unwrap();
        vec.push(self.routing.into()).unwrap();
        vec.push(self.signal.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.midi_in_ch.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_out_a.into()).unwrap();
        vec.push(self.midi_out_b.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    delay_saved: u16,
    feedback_saved: u16,
    interval_saved: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            delay_saved: 2048,
            feedback_saved: 0,
            interval_saved: 2048,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventKind {
    NoteOn,
    NoteOff,
    /// Delayed CV/CC level (MIDI→CV pitch or CV→MIDI CC).
    CvValue,
    GateHigh,
    GateLow,
}

#[derive(Clone, Copy)]
struct PendingEvent {
    kind: EventKind,
    base_note: u8,
    velocity: u16,
    cv_value: u16,
    interval: i8,
    out_target: u8,
    due_ms: u64,
    due_tick: u32,
    generation: u8,
}

fn fader_to_delay_ms(fader: u16, max_ms: i32) -> u64 {
    let max = max_ms.clamp(10, 2000) as u32;
    // Match clock-mode / rate UX: fader up = faster (shorter delay).
    let inverted = 4095u32.saturating_sub(fader as u32);
    (inverted * max / 4095) as u64
}

fn fader_to_interval(fader: u16) -> i8 {
    let centered = fader as i32 - 2048;
    ((centered * 12) / 2048).clamp(-12, 12) as i8
}

fn interval_for_gen(base: i8, generation: u8, mode: usize) -> i8 {
    match mode {
        1 => base.saturating_mul((generation as i8).saturating_add(1)),
        2 => {
            if generation.is_multiple_of(2) {
                base
            } else {
                base.saturating_neg()
            }
        }
        _ => base,
    }
}

fn out_target_for_gen(generation: u8, ping_pong: bool) -> u8 {
    if ping_pong && !generation.is_multiple_of(2) {
        1
    } else {
        0
    }
}

fn note_num(base_note: u8, interval: i8) -> u8 {
    (base_note as i16 + interval as i16).clamp(0, 127) as u8
}

fn note_to_cv(note: u8) -> u16 {
    let note_in = bits_7_16(u7::new(note.min(127)));
    ((note_in as u32 * 410) / 12).min(4095) as u16
}

fn midi_note_u8(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn split_semitone_leds(interval: i32) -> [u8; 2] {
    if interval >= 0 {
        let pos = ((interval * 255) / 12).clamp(0, 255) as u8;
        [pos, 0]
    } else {
        let neg = (((-interval) * 255) / 12).clamp(0, 255) as u8;
        [0, neg]
    }
}

/// Button/activity pulse brightness from feedback fader: 10%…100% of full scale.
fn pulse_from_feedback(feedback: u16) -> u8 {
    const MIN: u32 = 26; // ≈10% of 255
    const MAX: u32 = 255;
    (MIN + (feedback as u32 * (MAX - MIN) / 4095)) as u8
}

/// Button brightness from |interval|: 10% at unison … 100% at ±12 st.
fn pulse_from_interval(interval: i8) -> u8 {
    const MIN: u32 = 26;
    const MAX: u32 = 255;
    let mag = interval.unsigned_abs() as u32;
    (MIN + (mag * (MAX - MIN) / 12)) as u8
}

/// Resolve Signal enum in context of I/O mode.
fn effective_signal(io_mode: usize, signal: usize) -> usize {
    match io_mode {
        IO_MIDI_CV => {
            if signal == SIG_GATE {
                SIG_GATE
            } else {
                SIG_PITCH
            }
        }
        IO_CV_MIDI => {
            if signal == SIG_CV_CC {
                SIG_CV_CC
            } else {
                SIG_GATE_NOTE
            }
        }
        _ => SIG_PITCH, // unused in MIDI→MIDI
    }
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let ch = app.start_channel as u8;
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            io_mode: IO_MIDI_MIDI,
            delay_mode: 0,
            max_delay_ms: 1000,
            interval_mode: 0,
            routing: 0,
            signal: SIG_PITCH,
            range: Range::_0_10V,
            color: Color::Cyan,
            midi_in: MidiIn::default(),
            midi_in_ch: MidiChannel::default(),
            midi_out: MidiOut::default(),
            midi_out_a: MidiChannel::default(),
            midi_out_b: MidiChannel::from(2),
            midi_cc: MidiCc::from(32u8.saturating_add(ch)),
            midi_note: MidiNote::from(60),
        },
    );
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

    let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (
        io_mode,
        delay_mode,
        max_delay_ms,
        interval_mode,
        routing,
        signal,
        range,
        led_color,
        midi_in_cfg,
        midi_in_ch,
        midi_out_cfg,
        midi_out_a,
        midi_out_b,
        midi_cc,
        midi_note,
    ) = params.query(|p| {
        (
            p.io_mode,
            p.delay_mode,
            p.max_delay_ms,
            p.interval_mode,
            p.routing,
            p.signal,
            p.range,
            p.color,
            p.midi_in,
            p.midi_in_ch,
            p.midi_out,
            p.midi_out_a,
            p.midi_out_b,
            p.midi_cc,
            p.midi_note,
        )
    });

    let clocked = delay_mode == 1;
    let ping_pong = routing == 1 && io_mode == IO_MIDI_MIDI;
    let sig = effective_signal(io_mode, signal);
    let resolution = resolution_for_mode(CLOCK_DIVISION_MODE);
    let base_note_cfg = midi_note_u8(midi_note);

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();

    let mut midi_in = app.use_midi_input(midi_in_cfg, midi_in_ch);
    let midi_a = app.use_midi_output(midi_out_cfg, midi_out_a, false);
    let midi_b = app.use_midi_output(midi_out_cfg, midi_out_b, false);

    // One jack: out for MIDI→CV, in for CV→MIDI, unused for MIDI→MIDI.
    let out_jack = if io_mode == IO_MIDI_CV {
        Some(app.make_out_jack(0, range).await)
    } else {
        None
    };
    let in_jack = if io_mode == IO_CV_MIDI {
        Some(app.make_in_jack(0, range).await)
    } else {
        None
    };

    let glob_muted = app.make_global(false);
    let long_press_fired = app.make_global(false);
    let third_layer_used = app.make_global(false);
    let panic_flag = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let delay_saved_glob = app.make_global(2048u16);
    let feedback_glob = app.make_global(0u16);
    let interval_glob = app.make_global(0i8);
    let activity_glob = app.make_global(0u8);
    // White button flash on MIDI/CV note/gate input (proves listen path).
    let input_flash_glob = app.make_global(0u8);
    // 0 = Out A / top flash, 1 = Out B / bottom flash (ping-pong cue).
    let pong_side_glob = app.make_global(0u8);
    let queue_depth_glob = app.make_global(0u8);

    let (delay_saved, feedback_saved, interval_saved, muted) = storage.query(|s| {
        (
            s.delay_saved,
            s.feedback_saved,
            s.interval_saved,
            s.muted,
        )
    });
    delay_saved_glob.set(delay_saved);
    feedback_glob.set(feedback_saved);
    interval_glob.set(fader_to_interval(interval_saved));
    glob_muted.set(muted);

    if muted {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let engine = async {
        let mut queue: Vec<PendingEvent, QUEUE_CAP> = Vec::new();
        let mut sounding: Vec<(u8, u8), SOUNDING_CAP> = Vec::new();
        let mut prev_gate = false;
        let mut last_cc_gate: u16 = u16::MAX;
        // Free-running delay-period metronome (blinks even with empty queue).
        let mut next_metro_ms = Instant::now().as_millis();
        let mut next_metro_tick = ticks() as u32;

        let enqueue = |queue: &mut Vec<PendingEvent, QUEUE_CAP>,
                       kind: EventKind,
                       base_note: u8,
                       velocity: u16,
                       cv_value: u16,
                       generation: u8,
                       base_interval: i8,
                       delay_ms: u64,
                       delay_ticks: u32,
                       now_ms: u64,
                       now_tick: u32| {
            let _ = queue.push(PendingEvent {
                kind,
                base_note,
                velocity,
                cv_value,
                interval: interval_for_gen(base_interval, generation, interval_mode),
                out_target: out_target_for_gen(generation, ping_pong),
                due_ms: now_ms.saturating_add(delay_ms),
                due_tick: now_tick.wrapping_add(delay_ticks),
                generation,
            });
        };

        loop {
            // MIDI→MIDI / MIDI→CV: wait on MIDI or 1ms tick.
            // CV→MIDI: poll only (no MIDI input needed).
            let midi_msg = if io_mode == IO_CV_MIDI {
                app.delay_millis(1).await;
                None
            } else {
                match select(midi_in.wait_for_message(), app.delay_millis(1)).await {
                    Either::First(msg) => Some(msg),
                    Either::Second(_) => None,
                }
            };

            let delay_fader = delay_saved_glob.get();
            let base_interval = interval_glob.get();
            let now_ms = Instant::now().as_millis();
            let now_tick = ticks() as u32;
            let delay_ms = fader_to_delay_ms(delay_fader, max_delay_ms);
            let delay_ticks = value_to_resolution(delay_fader, resolution).max(1);
            let feedback = feedback_glob.get();
            let pulse = pulse_from_feedback(feedback);

            // Idle delay metronome: one blink per delay period, no input required.
            // Floor period so near-zero delay still reads as a pulse (~25 Hz max).
            const MIN_METRO_MS: u64 = 40;
            if !glob_muted.get() {
                if clocked {
                    if now_tick.wrapping_sub(next_metro_tick) < (u32::MAX / 2) {
                        activity_glob.set(pulse);
                        next_metro_tick = now_tick.wrapping_add(delay_ticks);
                    }
                } else if now_ms >= next_metro_ms {
                    activity_glob.set(pulse);
                    next_metro_ms = now_ms.saturating_add(delay_ms.max(MIN_METRO_MS));
                }
            }

            if let Some(msg) = midi_msg {
                let accept_new = !glob_muted.get();
                match (io_mode, sig, msg) {
                    // ── MIDI → MIDI notes ──────────────────────────────────
                    (
                        IO_MIDI_MIDI,
                        _,
                        MidiMessage::NoteOn { key, vel },
                    ) if vel > 0 => {
                        // Always flash on NoteOn so MIDI In is verifiable (even when muted).
                        input_flash_glob.set(INPUT_FLASH_PEAK);
                        if accept_new {
                            enqueue(
                                &mut queue,
                                EventKind::NoteOn,
                                key.as_int(),
                                scale_bits_7_12(vel),
                                0,
                                0,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                now_ms,
                                now_tick,
                            );
                        }
                    }
                    (IO_MIDI_MIDI, _, MidiMessage::NoteOn { key, .. })
                    | (IO_MIDI_MIDI, _, MidiMessage::NoteOff { key, .. }) => {
                        // Note-offs always accepted so held notes can release during ring-out.
                        enqueue(
                            &mut queue,
                            EventKind::NoteOff,
                            key.as_int(),
                            0,
                            0,
                            0,
                            base_interval,
                            delay_ms,
                            delay_ticks,
                            now_ms,
                            now_tick,
                        );
                    }

                    // ── MIDI → CV Pitch (hold last delayed note) ───────────
                    (
                        IO_MIDI_CV,
                        SIG_PITCH,
                        MidiMessage::NoteOn { key, vel },
                    ) if vel > 0 => {
                        input_flash_glob.set(INPUT_FLASH_PEAK);
                        if accept_new {
                            let n = note_num(
                                key.as_int(),
                                interval_for_gen(base_interval, 0, interval_mode),
                            );
                            enqueue(
                                &mut queue,
                                EventKind::CvValue,
                                key.as_int(),
                                scale_bits_7_12(vel),
                                note_to_cv(n),
                                0,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                now_ms,
                                now_tick,
                            );
                        }
                    }

                    // ── MIDI → CV Gate ─────────────────────────────────────
                    (
                        IO_MIDI_CV,
                        SIG_GATE,
                        MidiMessage::NoteOn { key, vel },
                    ) if vel > 0 => {
                        input_flash_glob.set(INPUT_FLASH_PEAK);
                        if accept_new {
                            enqueue(
                                &mut queue,
                                EventKind::GateHigh,
                                key.as_int(),
                                scale_bits_7_12(vel),
                                4095,
                                0,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                now_ms,
                                now_tick,
                            );
                        }
                    }
                    (IO_MIDI_CV, SIG_GATE, MidiMessage::NoteOn { key, .. })
                    | (IO_MIDI_CV, SIG_GATE, MidiMessage::NoteOff { key, .. }) => {
                        enqueue(
                            &mut queue,
                            EventKind::GateLow,
                            key.as_int(),
                            0,
                            0,
                            0,
                            base_interval,
                            delay_ms,
                            delay_ticks,
                            now_ms,
                            now_tick,
                        );
                    }
                    _ => {}
                }
            }

            // ── CV → MIDI input sampling ───────────────────────────────────
            if io_mode == IO_CV_MIDI {
                if let Some(jack) = in_jack.as_ref() {
                    let inval = jack.get_value();
                    let accept_new = !glob_muted.get();
                    if sig == SIG_GATE_NOTE {
                        let high = inval >= GATE_THRESH;
                        if high && !prev_gate {
                            input_flash_glob.set(INPUT_FLASH_PEAK);
                            if accept_new {
                                enqueue(
                                    &mut queue,
                                    EventKind::NoteOn,
                                    base_note_cfg,
                                    4095,
                                    0,
                                    0,
                                    base_interval,
                                    delay_ms,
                                    delay_ticks,
                                    now_ms,
                                    now_tick,
                                );
                            }
                        } else if !high && prev_gate {
                            enqueue(
                                &mut queue,
                                EventKind::NoteOff,
                                base_note_cfg,
                                0,
                                0,
                                0,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                now_ms,
                                now_tick,
                            );
                        }
                        prev_gate = high;
                    } else if sig == SIG_CV_CC && accept_new {
                        let g = midi_gate(inval, false);
                        if g != last_cc_gate {
                            last_cc_gate = g;
                            enqueue(
                                &mut queue,
                                EventKind::CvValue,
                                0,
                                0,
                                inval,
                                0,
                                0,
                                delay_ms,
                                delay_ticks,
                                now_ms,
                                now_tick,
                            );
                        }
                    }
                }
            }

            // ── Panic (hard kill) ──────────────────────────────────────────
            if panic_flag.get() {
                for (out, n) in sounding.iter() {
                    let note = MidiNote::from(*n);
                    if *out == 0 {
                        midi_a.send_note_off(note).await;
                    } else {
                        midi_b.send_note_off(note).await;
                    }
                }
                // Catch delayed NoteOns that never reached sounding yet.
                for event in queue.iter() {
                    if matches!(event.kind, EventKind::NoteOn) {
                        let n = note_num(event.base_note, event.interval);
                        let note = MidiNote::from(n);
                        if event.out_target == 0 {
                            midi_a.send_note_off(note).await;
                        } else {
                            midi_b.send_note_off(note).await;
                        }
                    }
                }
                // Channel-wide MIDI panic on both outs.
                const ALL_SOUND_OFF: u8 = 120;
                const ALL_NOTES_OFF: u8 = 123;
                midi_a
                    .send_cc(MidiCc::from(ALL_SOUND_OFF), 0)
                    .await;
                midi_a
                    .send_cc(MidiCc::from(ALL_NOTES_OFF), 0)
                    .await;
                midi_b
                    .send_cc(MidiCc::from(ALL_SOUND_OFF), 0)
                    .await;
                midi_b
                    .send_cc(MidiCc::from(ALL_NOTES_OFF), 0)
                    .await;

                sounding.clear();
                queue.clear();
                if let Some(jack) = out_jack.as_ref() {
                    jack.set_value(0);
                }
                prev_gate = false;
                last_cc_gate = u16::MAX;
                panic_flag.set(false);
                activity_glob.set(0);
                queue_depth_glob.set(0);
                continue;
            }

            // ── Due event processing (plays during mute = ring-out) ────────
            // Feedback only for note/gate event streams (not continuous CC/pitch holds).
            let feedback_ok = matches!(
                (io_mode, sig),
                (IO_MIDI_MIDI, _) | (IO_MIDI_CV, SIG_GATE) | (IO_CV_MIDI, SIG_GATE_NOTE)
            );

            let mut i = 0;
            while i < queue.len() {
                let due = if clocked {
                    now_tick.wrapping_sub(queue[i].due_tick) < (u32::MAX / 2)
                } else {
                    now_ms >= queue[i].due_ms
                };
                if !due {
                    i += 1;
                    continue;
                }

                let event = queue.swap_remove(i);
                let n = note_num(event.base_note, event.interval);
                let note = MidiNote::from(n);

                match event.kind {
                    EventKind::NoteOn => {
                        if event.out_target == 0 {
                            midi_a.send_note_on(note, event.velocity).await;
                        } else {
                            midi_b.send_note_on(note, event.velocity).await;
                        }
                        let _ = sounding.push((event.out_target, n));
                        activity_glob.set(pulse);
                        pong_side_glob.set(event.out_target);
                        if feedback_ok && feedback > 0 && event.generation < MAX_REPEATS {
                            let next_vel =
                                ((event.velocity as u32 * feedback as u32) / 4095) as u16;
                            if next_vel >= VELOCITY_FLOOR {
                                let next_gen = event.generation.saturating_add(1);
                                enqueue(
                                    &mut queue,
                                    EventKind::NoteOn,
                                    event.base_note,
                                    next_vel,
                                    0,
                                    next_gen,
                                    base_interval,
                                    delay_ms,
                                    delay_ticks,
                                    event.due_ms,
                                    event.due_tick,
                                );
                            }
                        }
                    }
                    EventKind::NoteOff => {
                        if event.out_target == 0 {
                            midi_a.send_note_off(note).await;
                        } else {
                            midi_b.send_note_off(note).await;
                        }
                        if let Some(pos) = sounding
                            .iter()
                            .position(|(o, sn)| *o == event.out_target && *sn == n)
                        {
                            sounding.swap_remove(pos);
                        }
                        if feedback_ok && feedback > 0 && event.generation < MAX_REPEATS {
                            let next_gen = event.generation.saturating_add(1);
                            enqueue(
                                &mut queue,
                                EventKind::NoteOff,
                                event.base_note,
                                0,
                                0,
                                next_gen,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                event.due_ms,
                                event.due_tick,
                            );
                        }
                    }
                    EventKind::CvValue => {
                        if io_mode == IO_MIDI_CV {
                            if let Some(jack) = out_jack.as_ref() {
                                jack.set_value(event.cv_value);
                            }
                        } else if io_mode == IO_CV_MIDI {
                            midi_a.send_cc(midi_cc, event.cv_value).await;
                        }
                        activity_glob.set(pulse);
                        pong_side_glob.set(0);
                    }
                    EventKind::GateHigh => {
                        if let Some(jack) = out_jack.as_ref() {
                            jack.set_value(4095);
                        }
                        activity_glob.set(pulse);
                        pong_side_glob.set(0);
                        if feedback_ok && feedback > 0 && event.generation < MAX_REPEATS {
                            let next_vel =
                                ((event.velocity as u32 * feedback as u32) / 4095) as u16;
                            if next_vel >= VELOCITY_FLOOR {
                                let next_gen = event.generation.saturating_add(1);
                                enqueue(
                                    &mut queue,
                                    EventKind::GateHigh,
                                    event.base_note,
                                    next_vel,
                                    4095,
                                    next_gen,
                                    base_interval,
                                    delay_ms,
                                    delay_ticks,
                                    event.due_ms,
                                    event.due_tick,
                                );
                            }
                        }
                    }
                    EventKind::GateLow => {
                        if let Some(jack) = out_jack.as_ref() {
                            jack.set_value(0);
                        }
                        if feedback_ok && feedback > 0 && event.generation < MAX_REPEATS {
                            let next_gen = event.generation.saturating_add(1);
                            enqueue(
                                &mut queue,
                                EventKind::GateLow,
                                event.base_note,
                                0,
                                0,
                                next_gen,
                                base_interval,
                                delay_ms,
                                delay_ticks,
                                event.due_ms,
                                event.due_tick,
                            );
                        }
                    }
                }
            }

            queue_depth_glob.set(
                ((queue.len() as u32 * 255) / QUEUE_CAP as u32).min(255) as u8,
            );

            if activity_glob.get() > 0 {
                // Decay ~32ms to black from full so each delay tick reads as a blink.
                activity_glob.set(activity_glob.get().saturating_sub(8));
            }
        }
    };

    let button_handler = async {
        loop {
            buttons.wait_for_any_down().await;
            if !buttons.is_shift_pressed() {
                long_press_fired.set(false);
                third_layer_used.set(false);
                buttons.wait_for_up(0).await;
                if long_press_fired.get() {
                    // Long-press panic only if this hold wasn't Interval (Button+Fader).
                    if !third_layer_used.get() {
                        panic_flag.set(true);
                    }
                } else if !third_layer_used.get() {
                    // Short press: mute / ring-out (block new input; queue keeps playing).
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| {
                        s.muted = muted;
                    });
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
            }
        }
    };

    let long_press = async {
        loop {
            // Only marks the gesture; panic runs on release (see button_handler)
            // so Button+Fader Interval edits never wipe the echo trail mid-hold.
            let _ = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
        }
    };

    let fader_handler = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;
            let latch_layer = glob_latch_layer.get();
            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.delay_saved),
                LatchLayer::Alt => storage.query(|s| s.feedback_saved),
                LatchLayer::Third => storage.query(|s| s.interval_saved),
            };
            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                if latch_layer == LatchLayer::Third {
                    third_layer_used.set(true);
                }
                match latch_layer {
                    LatchLayer::Main => {
                        delay_saved_glob.set(new_value);
                        storage.modify_and_save(|s| s.delay_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        feedback_glob.set(new_value);
                        storage.modify_and_save(|s| s.feedback_saved = new_value);
                    }
                    LatchLayer::Third => {
                        interval_glob.set(fader_to_interval(new_value));
                        storage.modify_and_save(|s| s.interval_saved = new_value);
                    }
                }
            }
        }
    };

    let led_handler = async {
        loop {
            app.delay_millis(1).await;
            let latch_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_layer);

            // Incoming-note flash (white) — shown even when muted, to diagnose MIDI In.
            // Muted: same decay envelope, peak shown at ~20% so mute still reads as muted.
            let input_flash = input_flash_glob.get();
            if input_flash > 0 {
                let shown = if glob_muted.get() {
                    ((input_flash as u16 * INPUT_FLASH_MUTED_SCALE) / 255) as u8
                } else {
                    input_flash
                };
                leds.set(
                    0,
                    Led::Button,
                    Color::White,
                    Brightness::Custom(shown.max(1)),
                );
                input_flash_glob.set(input_flash.saturating_sub(10));
            }

            if glob_muted.get() {
                if input_flash == 0 {
                    leds.unset(0, Led::Button);
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                }
                continue;
            }

            match latch_layer {
                LatchLayer::Main => {
                    let val = delay_saved_glob.get();
                    let led = split_unsigned_value(val);
                    let pulse = activity_glob.get();
                    let pong = pong_side_glob.get();
                    let depth = queue_depth_glob.get();

                    // Delay meter as baseline; delay-cycle pulse overlays.
                    // Ping-Pong: pulse hits Top for Out A, Bottom for Out B.
                    let (top_b, bot_b) = if pulse > 0 && ping_pong {
                        if pong == 0 {
                            (pulse, led[1].max(depth / 4))
                        } else {
                            (led[0].max(depth / 4), pulse)
                        }
                    } else {
                        (
                            led[0].max(pulse).max(depth / 4),
                            led[1]
                                .max(if pulse > 0 { pulse / 2 } else { 0 })
                                .max(depth / 4),
                        )
                    };
                    leds.set(0, Led::Top, led_color, Brightness::Custom(top_b));
                    leds.set(0, Led::Bottom, led_color, Brightness::Custom(bot_b));
                    // White input flash wins over delay pulse on the button.
                    if input_flash == 0 {
                        if pulse > 0 {
                            leds.set(0, Led::Button, led_color, Brightness::Custom(pulse));
                        } else {
                            leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                        }
                    }
                }
                LatchLayer::Alt => {
                    // Feedback: low = Bottom, high = Top (same split as delay meter).
                    let led = split_unsigned_value(feedback_glob.get());
                    let btn = pulse_from_feedback(feedback_glob.get());
                    leds.set(0, Led::Top, Color::Green, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Green, Brightness::Custom(led[1]));
                    leds.set(0, Led::Button, Color::Green, Brightness::Custom(btn));
                }
                LatchLayer::Third => {
                    // Interval: down = Bottom, up = Top; button tracks |st|.
                    let interval = interval_glob.get();
                    let led = split_semitone_leds(interval as i32);
                    let btn = pulse_from_interval(interval);
                    leds.set(0, Led::Top, Color::Red, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(led[1]));
                    leds.set(0, Led::Button, Color::Red, Brightness::Custom(btn));
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (delay_saved, feedback_saved, interval_saved, muted) = storage.query(|s| {
                        (
                            s.delay_saved,
                            s.feedback_saved,
                            s.interval_saved,
                            s.muted,
                        )
                    });
                    delay_saved_glob.set(delay_saved);
                    feedback_glob.set(feedback_saved);
                    interval_glob.set(fader_to_interval(interval_saved));
                    glob_muted.set(muted);
                    if muted {
                        leds.unset(0, Led::Button);
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                        panic_flag.set(true);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let clock_watch = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Stop | ClockEvent::Reset if clocked => {
                    panic_flag.set(true);
                }
                _ => {}
            }
        }
    };

    join(
        long_press,
        join5(
            engine,
            button_handler,
            fader_handler,
            join(led_handler, scene_handler),
            clock_watch,
        ),
    )
    .await;
}
