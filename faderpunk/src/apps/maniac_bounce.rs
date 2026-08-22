use embassy_futures::{
    join::{join, join4},
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Instant;
use heapless::Vec;
use libm;
use midly::{num::u7, MidiMessage};
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{bits_7_16, midi_gate, scale_bits_7_12, split_unsigned_value},
    AppIcon, Brightness, Color, Config, MidiCc, MidiChannel, MidiIn, MidiNote, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 11;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const BUTTON_DUCK_MS: u16 = 25;
const MAX_REPEATS: u8 = 16;
const VELOCITY_FLOOR: u16 = 200;
const QUEUE_CAP: usize = 48;
const QUEUE_FEEDBACK_RESERVE: usize = 4;
const SOUNDING_CAP: usize = 32;
const OPEN_NOTES_CAP: usize = 16;
const GATE_THRESH: u16 = 406;
const INPUT_FLASH_PEAK: u8 = 255;
const INPUT_FLASH_MUTED_SCALE: u16 = 51;
const LOOPBACK_IGNORE_MS: u64 = 40;
const RECENT_EMIT_CAP: usize = 24;
const MIN_DELAY_MS: u64 = 1;
const MAX_DELAY_MS_CAP: i32 = 8000;
const INTERVAL_ST_MAX: i32 = 24;
/// Default Alt fader: ~+6 semitones (nonzero bounce interval).
const DEFAULT_INTERVAL_SAVED: u16 = 2560;
/// Default Third fader: mid-high Vel LFO.
const DEFAULT_VEL_LFO_SAVED: u16 = 3072;

const IO_MIDI_MIDI: usize = 0;
const IO_MIDI_CV: usize = 1;
const IO_CV_MIDI: usize = 2;

const SIG_PITCH: usize = 0;
const SIG_GATE: usize = 1;
const SIG_CV_CC: usize = 2;
const SIG_GATE_NOTE: usize = 3;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Maniac Bounce",
    "Delay with bounce pitch and velocity LFO",
    Color::Pink,
    AppIcon::Sine,
)
.add_param(Param::Enum {
    name: "I/O",
    variants: &["MIDI->MIDI", "MIDI->CV", "CV->MIDI"],
})
.add_param(Param::i32 {
    name: "Max delay (ms)",
    min: 10,
    max: 8000,
})
.add_param(Param::Enum {
    name: "Signal",
    variants: &["Pitch", "Gate", "CV->CC", "Gate->Note"],
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
.add_param(Param::MidiChannel { name: "MIDI In CH" })
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel { name: "MIDI Out" })
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiNote { name: "MIDI Note" });

pub struct Params {
    io_mode: usize,
    max_delay_ms: i32,
    signal: usize,
    range: Range,
    color: Color,
    midi_in: MidiIn,
    midi_in_ch: MidiChannel,
    midi_out: MidiOut,
    midi_out_ch: MidiChannel,
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
            max_delay_ms: i32::from_value(values[1]),
            signal: usize::from_value(values[2]),
            range: Range::from_value(values[3]),
            color: Color::from_value(values[4]),
            midi_in: MidiIn::from_value(values[5]),
            midi_in_ch: MidiChannel::from_value(values[6]),
            midi_out: MidiOut::from_value(values[7]),
            midi_out_ch: MidiChannel::from_value(values[8]),
            midi_cc: MidiCc::from_value(values[9]),
            midi_note: MidiNote::from_value(values[10]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.io_mode.into()).unwrap();
        vec.push(self.max_delay_ms.into()).unwrap();
        vec.push(self.signal.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.midi_in_ch.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_out_ch.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    delay_saved: u16,
    interval_saved: u16,
    vel_lfo_saved: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            delay_saved: 2048,
            interval_saved: DEFAULT_INTERVAL_SAVED,
            vel_lfo_saved: DEFAULT_VEL_LFO_SAVED,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventKind {
    NoteOn,
    NoteOff,
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
    due_ms: u64,
    generation: u8,
    delay_ms: u64,
    base_interval: i8,
    is_dry: bool,
}

fn fader_to_delay_ms(fader: u16, max_ms: i32) -> u64 {
    let max = max_ms.clamp(10, MAX_DELAY_MS_CAP) as u32;
    let inverted = 4095u32.saturating_sub(fader as u32);
    ((inverted * max / 4095) as u64).max(MIN_DELAY_MS)
}

fn feedback_retention(fader: u16) -> u16 {
    if fader == 0 {
        return 0;
    }
    let x = fader as u32;
    let inv = 4095u32.saturating_sub(x);
    (4095u32.saturating_sub((inv * inv) / 4095)) as u16
}

/// Main fader couples delay length with echo retention (short delay -> fewer hops).
fn main_coupled_retention(delay_fader: u16) -> u16 {
    let inverted = 4095u16.saturating_sub(delay_fader);
    feedback_retention(inverted)
}

fn next_repeat_velocity(velocity: u16, delay_fader: u16) -> Option<u16> {
    let retention = main_coupled_retention(delay_fader);
    if retention == 0 {
        return None;
    }
    let next = ((velocity as u32 * retention as u32) / 4095) as u16;
    (next >= VELOCITY_FLOOR).then_some(next)
}

fn ports_can_loop(midi_in: MidiIn, midi_out: MidiOut) -> bool {
    let MidiIn([usb_in, din_in]) = midi_in;
    let MidiOut([usb_out, din1_out, din2_out]) = midi_out;
    (usb_in && usb_out) || (din_in && (din1_out || din2_out))
}

fn same_channel_loop_risk(midi_in_ch: MidiChannel, midi_out_ch: MidiChannel) -> bool {
    midi_in_ch == midi_out_ch
}

#[derive(Clone, Copy)]
struct RecentEmit {
    note: u8,
    is_on: bool,
    at_ms: u64,
}

fn record_emit(recent: &mut Vec<RecentEmit, RECENT_EMIT_CAP>, note: u8, is_on: bool, now_ms: u64) {
    let mut i = 0;
    while i < recent.len() {
        if now_ms.saturating_sub(recent[i].at_ms) >= LOOPBACK_IGNORE_MS {
            recent.swap_remove(i);
        } else {
            i += 1;
        }
    }
    if recent.is_full() {
        let _ = recent.remove(0);
    }
    let _ = recent.push(RecentEmit {
        note,
        is_on,
        at_ms: now_ms,
    });
}

fn is_own_echo(recent: &[RecentEmit], note: u8, is_on: bool, now_ms: u64) -> bool {
    recent.iter().any(|e| {
        e.note == note && e.is_on == is_on && now_ms.saturating_sub(e.at_ms) < LOOPBACK_IGNORE_MS
    })
}

fn fader_to_interval(fader: u16) -> i8 {
    let centered = fader as i32 - 2048;
    ((centered * INTERVAL_ST_MAX) / 2048).clamp(-INTERVAL_ST_MAX, INTERVAL_ST_MAX) as i8
}

/// Bounce only: even generation = +interval, odd = -interval.
fn bounce_interval(base: i8, generation: u8) -> i8 {
    if generation.is_multiple_of(2) {
        base
    } else {
        base.saturating_neg()
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
        let pos = ((interval * 255) / INTERVAL_ST_MAX).clamp(0, 255) as u8;
        [pos, 0]
    } else {
        let neg = (((-interval) * 255) / INTERVAL_ST_MAX).clamp(0, 255) as u8;
        [0, neg]
    }
}

fn vel_lfo_params(fader: u16) -> (f32, f32) {
    let t = fader as f32 / 4095.0;
    let rate = 0.5 + t * 11.5;
    let depth = t * t;
    (rate, depth)
}

fn apply_vel_lfo(base: u16, phase: f32, depth: f32) -> u16 {
    if depth <= 0.0 {
        return base;
    }
    let lfo = (libm::sinf(phase) + 1.0) * 0.5;
    let scale = 1.0 - depth + depth * lfo;
    ((base as f32 * scale).max(VELOCITY_FLOOR as f32)) as u16
}

fn gate_pulse_factor(phase: f32, depth: f32) -> f32 {
    if depth <= 0.0 {
        return 1.0;
    }
    let lfo = (libm::sinf(phase) + 1.0) * 0.5;
    0.2 + 0.8 * (1.0 - depth + depth * lfo)
}

fn pulse_from_interval(interval: i8) -> u8 {
    const MIN: u32 = 26;
    const MAX: u32 = 255;
    let mag = interval.unsigned_abs() as u32;
    (MIN + (mag * (MAX - MIN) / INTERVAL_ST_MAX as u32)) as u8
}

fn pulse_from_vel_lfo(fader: u16) -> u8 {
    const MIN: u32 = 26;
    const MAX: u32 = 255;
    (MIN + (fader as u32 * (MAX - MIN) / 4095)) as u8
}

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
        _ => SIG_PITCH,
    }
}

fn note_length_ms(delay_ms: u64) -> u64 {
    (delay_ms / 2).max(1)
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let ch = app.start_channel as u8;
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            io_mode: IO_MIDI_MIDI,
            max_delay_ms: 2000,
            signal: SIG_PITCH,
            range: Range::_0_10V,
            color: Color::Pink,
            midi_in: MidiIn::default(),
            midi_in_ch: MidiChannel::default(),
            midi_out: MidiOut::default(),
            midi_out_ch: MidiChannel::from(2),
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
        max_delay_ms,
        signal,
        range,
        led_color,
        midi_in_cfg,
        midi_in_ch,
        midi_out_cfg,
        midi_out_ch,
        midi_cc,
        midi_note,
    ) = params.query(|p| {
        (
            p.io_mode,
            p.max_delay_ms,
            p.signal,
            p.range,
            p.color,
            p.midi_in,
            p.midi_in_ch,
            p.midi_out,
            p.midi_out_ch,
            p.midi_cc,
            p.midi_note,
        )
    });

    let sig = effective_signal(io_mode, signal);
    let base_note_cfg = midi_note_u8(midi_note);
    let loop_guard = io_mode == IO_MIDI_MIDI
        && ports_can_loop(midi_in_cfg, midi_out_cfg)
        && same_channel_loop_risk(midi_in_ch, midi_out_ch);

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let mut midi_in = app.use_midi_input(midi_in_cfg, midi_in_ch);
    let midi_out = app.use_midi_output(midi_out_cfg, midi_out_ch, false);

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
    let interval_glob = app.make_global(0i8);
    let vel_lfo_glob = app.make_global(DEFAULT_VEL_LFO_SAVED);
    let activity_glob = app.make_global(0u8);
    let button_duck_glob = app.make_global(0u16);
    let input_flash_glob = app.make_global(0u8);
    let queue_depth_glob = app.make_global(0u8);
    let lfo_phase_glob = app.make_global(0.0f32);

    let (delay_saved, interval_saved, vel_lfo_saved, muted) =
        storage.query(|s| (s.delay_saved, s.interval_saved, s.vel_lfo_saved, s.muted));
    delay_saved_glob.set(delay_saved);
    interval_glob.set(fader_to_interval(interval_saved));
    vel_lfo_glob.set(vel_lfo_saved);
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
        let mut open_notes: Vec<(u8, u64, i8), OPEN_NOTES_CAP> = Vec::new();
        let mut open_gate_delay: Option<(u64, i8)> = None;
        let mut recent_emit: Vec<RecentEmit, RECENT_EMIT_CAP> = Vec::new();
        let mut prev_gate = false;
        let mut last_cc_gate: u16 = u16::MAX;
        let mut lfo_phase = 0.0f32;

        let enqueue = |queue: &mut Vec<PendingEvent, QUEUE_CAP>,
                       kind: EventKind,
                       base_note: u8,
                       velocity: u16,
                       cv_value: u16,
                       generation: u8,
                       base_interval: i8,
                       delay_ms: u64,
                       now_ms: u64,
                       is_dry: bool|
         -> bool {
            let is_release = matches!(kind, EventKind::NoteOff | EventKind::GateLow);
            if generation > 0 && !is_release && queue.len() + QUEUE_FEEDBACK_RESERVE >= QUEUE_CAP {
                return false;
            }
            if queue.is_full() {
                if is_release {
                    if let Some(pos) = queue.iter().position(|e| {
                        e.generation > 0
                            && matches!(e.kind, EventKind::NoteOn | EventKind::GateHigh)
                    }) {
                        queue.swap_remove(pos);
                    } else if let Some(pos) = queue
                        .iter()
                        .position(|e| matches!(e.kind, EventKind::NoteOn | EventKind::GateHigh))
                    {
                        queue.swap_remove(pos);
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            queue
                .push(PendingEvent {
                    kind,
                    base_note,
                    velocity,
                    cv_value,
                    interval: bounce_interval(base_interval, generation),
                    due_ms: now_ms.saturating_add(delay_ms),
                    generation,
                    delay_ms,
                    base_interval,
                    is_dry,
                })
                .is_ok()
        };

        let schedule_note_off = |queue: &mut Vec<PendingEvent, QUEUE_CAP>,
                                 base_note: u8,
                                 _interval: i8,
                                 delay_ms: u64,
                                 due_ms: u64,
                                 generation: u8,
                                 base_interval: i8| {
            let off_ms = note_length_ms(delay_ms);
            let _ = enqueue(
                queue,
                EventKind::NoteOff,
                base_note,
                0,
                0,
                generation,
                base_interval,
                off_ms,
                due_ms,
                false,
            );
        };

        loop {
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
            let vel_lfo_fader = vel_lfo_glob.get();
            let now_ms = Instant::now().as_millis();
            let delay_ms = fader_to_delay_ms(delay_fader, max_delay_ms);
            let (lfo_rate, lfo_depth) = vel_lfo_params(vel_lfo_fader);
            lfo_phase += lfo_rate * 0.001 * core::f32::consts::TAU;
            lfo_phase_glob.set(lfo_phase);
            let pulse = pulse_from_vel_lfo(vel_lfo_fader);

            let kill_pitch = |queue: &mut Vec<PendingEvent, QUEUE_CAP>,
                              sounding: &mut Vec<(u8, u8), SOUNDING_CAP>,
                              base_note: u8| {
                queue.retain(|e| e.base_note != base_note);
                sounding.retain(|(_, b)| *b != base_note);
            };

            if let Some(msg) = midi_msg {
                let accept_new = !glob_muted.get();
                match (io_mode, sig, msg) {
                    (IO_MIDI_MIDI, _, MidiMessage::NoteOn { key, vel }) if vel > 0 => {
                        let key_n = key.as_int();
                        if loop_guard && is_own_echo(&recent_emit, key_n, true, now_ms) {
                            input_flash_glob.set(INPUT_FLASH_PEAK);
                        } else {
                            input_flash_glob.set(INPUT_FLASH_PEAK);
                            if accept_new {
                                for (note, base) in sounding.iter().copied().collect::<Vec<_, 8>>()
                                {
                                    if base == key_n {
                                        midi_out.send_note_off(MidiNote::from(note)).await;
                                    }
                                }
                                kill_pitch(&mut queue, &mut sounding, key_n);

                                if let Some(slot) =
                                    open_notes.iter_mut().find(|(n, _, _)| *n == key_n)
                                {
                                    *slot = (key_n, delay_ms, base_interval);
                                } else {
                                    let _ = open_notes.push((key_n, delay_ms, base_interval));
                                }

                                let vel12 = scale_bits_7_12(vel);
                                let mod_vel = apply_vel_lfo(vel12, lfo_phase, lfo_depth);
                                midi_out.send_note_on(MidiNote::from(key_n), mod_vel).await;
                                record_emit(&mut recent_emit, key_n, true, now_ms);
                                let _ = sounding.push((key_n, key_n));
                                activity_glob.set(pulse);
                                button_duck_glob.set(BUTTON_DUCK_MS);

                                let dry_len = note_length_ms(delay_ms);
                                let _ = enqueue(
                                    &mut queue,
                                    EventKind::NoteOff,
                                    key_n,
                                    0,
                                    0,
                                    0,
                                    base_interval,
                                    dry_len,
                                    now_ms,
                                    true,
                                );

                                let _ = enqueue(
                                    &mut queue,
                                    EventKind::NoteOn,
                                    key_n,
                                    mod_vel,
                                    0,
                                    0,
                                    base_interval,
                                    delay_ms,
                                    now_ms,
                                    false,
                                );
                                schedule_note_off(
                                    &mut queue,
                                    key_n,
                                    bounce_interval(base_interval, 0),
                                    delay_ms,
                                    now_ms.saturating_add(delay_ms),
                                    0,
                                    base_interval,
                                );
                            }
                        }
                    }
                    (IO_MIDI_MIDI, _, MidiMessage::NoteOn { key, .. })
                    | (IO_MIDI_MIDI, _, MidiMessage::NoteOff { key, .. }) => {
                        let key_n = key.as_int();
                        if !(loop_guard && is_own_echo(&recent_emit, key_n, false, now_ms)) {
                            if let Some(pos) = open_notes.iter().position(|(n, _, _)| *n == key_n)
                            {
                                open_notes.swap_remove(pos);
                            }
                            // End dry only; bounce trail keeps auto-gating.
                            if let Some(pos) = sounding.iter().position(|(n, b)| *n == key_n && *b == key_n)
                            {
                                midi_out.send_note_off(MidiNote::from(key_n)).await;
                                sounding.swap_remove(pos);
                                record_emit(&mut recent_emit, key_n, false, now_ms);
                            }
                        }
                    }

                    (IO_MIDI_CV, SIG_PITCH, MidiMessage::NoteOn { key, vel }) if vel > 0 => {
                        input_flash_glob.set(INPUT_FLASH_PEAK);
                        if accept_new {
                            let key_n = key.as_int();
                            if let Some(jack) = out_jack.as_ref() {
                                jack.set_value(note_to_cv(key_n));
                            }
                            activity_glob.set(pulse);
                            let n = note_num(key_n, bounce_interval(base_interval, 0));
                            let _ = enqueue(
                                &mut queue,
                                EventKind::CvValue,
                                key_n,
                                scale_bits_7_12(vel),
                                note_to_cv(n),
                                0,
                                base_interval,
                                delay_ms,
                                now_ms,
                                false,
                            );
                        }
                    }

                    (IO_MIDI_CV, SIG_GATE, MidiMessage::NoteOn { key, vel }) if vel > 0 => {
                        input_flash_glob.set(INPUT_FLASH_PEAK);
                        if accept_new {
                            let key_n = key.as_int();
                            kill_pitch(&mut queue, &mut sounding, key_n);
                            if let Some(slot) = open_notes.iter_mut().find(|(n, _, _)| *n == key_n)
                            {
                                *slot = (key_n, delay_ms, base_interval);
                            } else {
                                let _ = open_notes.push((key_n, delay_ms, base_interval));
                            }
                            if let Some(jack) = out_jack.as_ref() {
                                jack.set_value(4095);
                            }
                            activity_glob.set(pulse);
                            let pulse_ms = ((note_length_ms(delay_ms) as f32)
                                * gate_pulse_factor(lfo_phase, lfo_depth))
                                as u64;
                            let _ = enqueue(
                                &mut queue,
                                EventKind::GateLow,
                                key_n,
                                0,
                                0,
                                0,
                                base_interval,
                                pulse_ms.max(1),
                                now_ms,
                                true,
                            );
                            let _ = enqueue(
                                &mut queue,
                                EventKind::GateHigh,
                                key_n,
                                scale_bits_7_12(vel),
                                0,
                                0,
                                base_interval,
                                delay_ms,
                                now_ms,
                                false,
                            );
                        }
                    }
                    (IO_MIDI_CV, SIG_GATE, MidiMessage::NoteOn { key, .. })
                    | (IO_MIDI_CV, SIG_GATE, MidiMessage::NoteOff { key, .. }) => {
                        let key_n = key.as_int();
                        if let Some(pos) = open_notes.iter().position(|(n, _, _)| *n == key_n) {
                            open_notes.swap_remove(pos);
                        }
                        if let Some(jack) = out_jack.as_ref() {
                            jack.set_value(0);
                        }
                    }
                    _ => {}
                }
            }

            if io_mode == IO_CV_MIDI {
                if let Some(jack) = in_jack.as_ref() {
                    let inval = jack.get_value();
                    let accept_new = !glob_muted.get();
                    if sig == SIG_GATE_NOTE {
                        let high = inval >= GATE_THRESH;
                        if high && !prev_gate {
                            input_flash_glob.set(INPUT_FLASH_PEAK);
                            if accept_new {
                                for (note, base) in sounding.iter().copied().collect::<Vec<_, 8>>()
                                {
                                    if base == base_note_cfg {
                                        midi_out.send_note_off(MidiNote::from(note)).await;
                                    }
                                }
                                kill_pitch(&mut queue, &mut sounding, base_note_cfg);
                                open_gate_delay = Some((delay_ms, base_interval));

                                let mod_vel = apply_vel_lfo(4095, lfo_phase, lfo_depth);
                                midi_out
                                    .send_note_on(MidiNote::from(base_note_cfg), mod_vel)
                                    .await;
                                record_emit(&mut recent_emit, base_note_cfg, true, now_ms);
                                let _ = sounding.push((base_note_cfg, base_note_cfg));
                                activity_glob.set(pulse);

                                let dry_len = note_length_ms(delay_ms);
                                let _ = enqueue(
                                    &mut queue,
                                    EventKind::NoteOff,
                                    base_note_cfg,
                                    0,
                                    0,
                                    0,
                                    base_interval,
                                    dry_len,
                                    now_ms,
                                    true,
                                );
                                let _ = enqueue(
                                    &mut queue,
                                    EventKind::NoteOn,
                                    base_note_cfg,
                                    mod_vel,
                                    0,
                                    0,
                                    base_interval,
                                    delay_ms,
                                    now_ms,
                                    false,
                                );
                                schedule_note_off(
                                    &mut queue,
                                    base_note_cfg,
                                    bounce_interval(base_interval, 0),
                                    delay_ms,
                                    now_ms.saturating_add(delay_ms),
                                    0,
                                    base_interval,
                                );
                            }
                        } else if !high && prev_gate {
                            let _ = open_gate_delay.take();
                            if let Some(pos) = sounding
                                .iter()
                                .position(|(n, b)| *n == base_note_cfg && *b == base_note_cfg)
                            {
                                midi_out.send_note_off(MidiNote::from(base_note_cfg)).await;
                                sounding.swap_remove(pos);
                                record_emit(&mut recent_emit, base_note_cfg, false, now_ms);
                            }
                        }
                        prev_gate = high;
                    } else if sig == SIG_CV_CC && accept_new {
                        let g = midi_gate(inval, false);
                        if g != last_cc_gate {
                            last_cc_gate = g;
                            let _ = enqueue(
                                &mut queue,
                                EventKind::CvValue,
                                0,
                                0,
                                inval,
                                0,
                                0,
                                delay_ms,
                                now_ms,
                                false,
                            );
                        }
                    }
                }
            }

            if panic_flag.get() {
                for (note, _) in sounding.iter() {
                    midi_out.send_note_off(MidiNote::from(*note)).await;
                }
                for event in queue.iter() {
                    if matches!(event.kind, EventKind::NoteOn) {
                        let n = note_num(event.base_note, event.interval);
                        midi_out.send_note_off(MidiNote::from(n)).await;
                    }
                }
                const ALL_SOUND_OFF: u8 = 120;
                const ALL_NOTES_OFF: u8 = 123;
                midi_out.send_cc(MidiCc::from(ALL_SOUND_OFF), 0).await;
                midi_out.send_cc(MidiCc::from(ALL_NOTES_OFF), 0).await;

                sounding.clear();
                queue.clear();
                open_notes.clear();
                open_gate_delay = None;
                recent_emit.clear();
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

            let repeat_ok = matches!(
                (io_mode, sig),
                (IO_MIDI_MIDI, _) | (IO_MIDI_CV, SIG_GATE) | (IO_CV_MIDI, SIG_GATE_NOTE)
            );

            loop {
                let mut best: Option<usize> = None;
                for i in 0..queue.len() {
                    if now_ms < queue[i].due_ms {
                        continue;
                    }
                    best = Some(match best {
                        None => i,
                        Some(j) => {
                            let earlier = match queue[i].due_ms.cmp(&queue[j].due_ms) {
                                core::cmp::Ordering::Less => true,
                                core::cmp::Ordering::Greater => false,
                                core::cmp::Ordering::Equal => matches!(
                                    (queue[i].kind, queue[j].kind),
                                    (EventKind::NoteOff, EventKind::NoteOn)
                                        | (EventKind::GateLow, EventKind::GateHigh)
                                ),
                            };
                            if earlier {
                                i
                            } else {
                                j
                            }
                        }
                    });
                }
                let Some(idx) = best else {
                    break;
                };
                let event = queue.swap_remove(idx);
                let n = note_num(event.base_note, event.interval);
                let note = MidiNote::from(n);
                let mod_vel = apply_vel_lfo(event.velocity, lfo_phase, lfo_depth);

                match event.kind {
                    EventKind::NoteOn => {
                        if sounding.iter().any(|(sn, _)| *sn == n) {
                            midi_out.send_note_off(note).await;
                            if let Some(pos) = sounding.iter().position(|(sn, _)| *sn == n) {
                                sounding.swap_remove(pos);
                            }
                        }
                        midi_out.send_note_on(note, mod_vel).await;
                        record_emit(&mut recent_emit, n, true, now_ms);
                        let _ = sounding.push((n, event.base_note));
                        activity_glob.set(pulse);
                        button_duck_glob.set(BUTTON_DUCK_MS);
                        schedule_note_off(
                            &mut queue,
                            event.base_note,
                            event.interval,
                            event.delay_ms,
                            event.due_ms,
                            event.generation,
                            event.base_interval,
                        );
                        if repeat_ok && !event.is_dry && event.generation < MAX_REPEATS {
                            if let Some(next_vel) = next_repeat_velocity(mod_vel, delay_fader) {
                                let next_gen = event.generation.saturating_add(1);
                                enqueue(
                                    &mut queue,
                                    EventKind::NoteOn,
                                    event.base_note,
                                    next_vel,
                                    0,
                                    next_gen,
                                    event.base_interval,
                                    event.delay_ms,
                                    event.due_ms,
                                    false,
                                );
                                schedule_note_off(
                                    &mut queue,
                                    event.base_note,
                                    bounce_interval(event.base_interval, next_gen),
                                    event.delay_ms,
                                    event.due_ms.saturating_add(event.delay_ms),
                                    next_gen,
                                    event.base_interval,
                                );
                            }
                        }
                    }
                    EventKind::NoteOff => {
                        midi_out.send_note_off(note).await;
                        record_emit(&mut recent_emit, n, false, now_ms);
                        if let Some(pos) = sounding.iter().position(|(sn, _)| *sn == n) {
                            sounding.swap_remove(pos);
                        }
                    }
                    EventKind::CvValue => {
                        if io_mode == IO_MIDI_CV {
                            if let Some(jack) = out_jack.as_ref() {
                                jack.set_value(event.cv_value);
                            }
                        } else if io_mode == IO_CV_MIDI {
                            midi_out.send_cc(midi_cc, event.cv_value).await;
                        }
                        activity_glob.set(pulse);
                        button_duck_glob.set(BUTTON_DUCK_MS);
                        if io_mode == IO_MIDI_CV
                            && !event.is_dry
                            && event.generation < MAX_REPEATS
                        {
                            if let Some(next_vel) =
                                next_repeat_velocity(event.velocity, delay_fader)
                            {
                                let next_gen = event.generation.saturating_add(1);
                                let bounced = note_num(
                                    event.base_note,
                                    bounce_interval(event.base_interval, next_gen),
                                );
                                enqueue(
                                    &mut queue,
                                    EventKind::CvValue,
                                    event.base_note,
                                    next_vel,
                                    note_to_cv(bounced),
                                    next_gen,
                                    event.base_interval,
                                    event.delay_ms,
                                    event.due_ms,
                                    false,
                                );
                            }
                        }
                    }
                    EventKind::GateHigh => {
                        if let Some(jack) = out_jack.as_ref() {
                            jack.set_value(4095);
                        }
                        activity_glob.set(pulse);
                        button_duck_glob.set(BUTTON_DUCK_MS);
                        let pulse_ms = ((note_length_ms(event.delay_ms) as f32)
                            * gate_pulse_factor(lfo_phase, lfo_depth))
                            as u64;
                        let _ = enqueue(
                            &mut queue,
                            EventKind::GateLow,
                            event.base_note,
                            0,
                            0,
                            event.generation,
                            event.base_interval,
                            pulse_ms.max(1),
                            event.due_ms,
                            false,
                        );
                        if repeat_ok && event.generation < MAX_REPEATS {
                            if let Some(next_vel) =
                                next_repeat_velocity(event.velocity, delay_fader)
                            {
                                let next_gen = event.generation.saturating_add(1);
                                enqueue(
                                    &mut queue,
                                    EventKind::GateHigh,
                                    event.base_note,
                                    next_vel,
                                    0,
                                    next_gen,
                                    event.base_interval,
                                    event.delay_ms,
                                    event.due_ms,
                                    false,
                                );
                            }
                        }
                    }
                    EventKind::GateLow => {
                        if let Some(jack) = out_jack.as_ref() {
                            jack.set_value(0);
                        }
                    }
                }
            }

            queue_depth_glob.set(((queue.len() as u32 * 255) / QUEUE_CAP as u32).min(255) as u8);

            if activity_glob.get() > 0 {
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
                    if !third_layer_used.get() {
                        panic_flag.set(true);
                    }
                } else if !third_layer_used.get() {
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
                LatchLayer::Alt => storage.query(|s| s.interval_saved),
                LatchLayer::Third => storage.query(|s| s.vel_lfo_saved),
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
                        interval_glob.set(fader_to_interval(new_value));
                        storage.modify_and_save(|s| s.interval_saved = new_value);
                    }
                    LatchLayer::Third => {
                        vel_lfo_glob.set(new_value);
                        storage.modify_and_save(|s| s.vel_lfo_saved = new_value);
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

            let duck = button_duck_glob.get();
            if duck > 0 {
                button_duck_glob.set(duck.saturating_sub(1));
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
                    let depth = queue_depth_glob.get();
                    leds.set(
                        0,
                        Led::Top,
                        led_color,
                        Brightness::Custom(led[0].max(pulse).max(depth / 4)),
                    );
                    leds.set(
                        0,
                        Led::Bottom,
                        led_color,
                        Brightness::Custom(led[1].max(pulse / 2).max(depth / 4)),
                    );
                    if input_flash == 0 {
                        let bright = if duck > 0 {
                            Brightness::Low
                        } else {
                            LED_BRIGHTNESS
                        };
                        leds.set(0, Led::Button, led_color, bright);
                    }
                }
                LatchLayer::Alt => {
                    let interval = interval_glob.get();
                    let led = split_semitone_leds(interval as i32);
                    let btn = pulse_from_interval(interval);
                    leds.set(0, Led::Top, Color::Orange, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Orange, Brightness::Custom(led[1]));
                    leds.set(0, Led::Button, Color::Orange, Brightness::Custom(btn));
                }
                LatchLayer::Third => {
                    let val = vel_lfo_glob.get();
                    let led = split_unsigned_value(val);
                    let btn = pulse_from_vel_lfo(val);
                    leds.set(0, Led::Top, Color::Violet, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Violet, Brightness::Custom(led[1]));
                    leds.set(0, Led::Button, Color::Violet, Brightness::Custom(btn));
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (delay_saved, interval_saved, vel_lfo_saved, muted) = storage
                        .query(|s| (s.delay_saved, s.interval_saved, s.vel_lfo_saved, s.muted));
                    delay_saved_glob.set(delay_saved);
                    interval_glob.set(fader_to_interval(interval_saved));
                    vel_lfo_glob.set(vel_lfo_saved);
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

    join(
        long_press,
        join4(
            engine,
            button_handler,
            fader_handler,
            join(led_handler, scene_handler),
        ),
    )
    .await;
}
