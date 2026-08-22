//! Loop de Cay — clocked additive overdub loop with level decay.
//!
//! Gestures:
//! - Press/hold = gate/play (Pitch→MIDI / Fader→Both); release ends the note.
//! - Hold+Fader = scale-degree glissando toward the fader (like Chord Vamp perform).
//! - Shift+Short = cycle loop length 1→8 bars.
//! - Shift+Long = mute (decay pauses); again while muted = erase buffer.
//! - Short while muted = unmute (re-arms recording).
//! - Shift+Fader = decay strength.
//! - Btn+Fader (Third; gate modes while muted) = decay mode.
//!
//! Pitch (live + decay) snaps to the global quantizer key/tonic (nearest degree).
//! I/O Mode and Decay Mode via Config.

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
    utils::split_unsigned_value,
    AppIcon, Brightness, ClockDivision, Color, Config, Key, MidiChannel, MidiIn, MidiNote, MidiOut,
    Note, Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppMidiEvent, AppParams, AppStorage, ClockEvent, Global, InJack, Led, ManagedStorage,
    MidiOutput, OutJack, ParamStore, Quantizer, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 10;

const TICKS_PER_16TH: u64 = 6;
const TICKS_PER_BAR: u16 = 96;
const MAX_BARS: u8 = 8;
const MAX_BUFFER_TICKS: u16 = TICKS_PER_BAR * MAX_BARS as u16;
const MAX_EVENTS: usize = 32;
const POLY: usize = 4;
const KILL_FLOOR: u16 = 96;
const FADER_MOVE_THRESH: u16 = 48;
const GATE_THRESH: u16 = 406;
const DEFAULT_GATE_TICKS: u16 = 12;
const ARM_BLINK_MS: u64 = 400;
/// Playback/capture hit: keep button dim this many PPQN ticks (sharp, short).
const CAPTURE_FLASH_TICKS: u64 = 2;
/// Remaining brightness % during capture dim (80% dim).
const CAPTURE_DIM_REMAIN_PCT: u16 = 20;
/// Hold length for Shift+Long mute/erase (matches hardware LONG_PRESS).
const SHIFT_LONG_MS: u64 = 500;
/// Hold+Fader glissando: engine frames (~1 ms) between scale-degree steps.
const GLISS_FRAMES: u16 = 35;

const DECAY_MODE_COUNT: usize = 7;
const DECAY_RANDOM: usize = 0;
const DECAY_PITCH_UP: usize = 1;
const DECAY_PITCH_DOWN: usize = 2;
const DECAY_VEL_ROOT: usize = 3;
const DECAY_PITCH_ROOT: usize = 4;
const DECAY_VEL: usize = 5;
const DECAY_GATE: usize = 6;
const DECAY_DEFAULT: usize = DECAY_VEL_ROOT;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Loop de Cay",
    "Additive overdub loop with level decay",
    Color::Violet,
    AppIcon::Sequence,
)
.add_param(Param::Enum {
    name: "Mode",
    variants: &[
        "Pitch->MIDI",
        "Gate->MIDI",
        "MIDI->MIDI",
        "MIDI->CV",
        "Fader->Both",
    ],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiIn)
.add_param(Param::MidiOut)
.add_param(Param::MidiNote {
    name: "Base Note",
})
.add_param(Param::i32 {
    name: "Span",
    min: 1,
    max: 120,
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
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::VoltPerOct)
.add_param(Param::Enum {
    name: "Decay Mode",
    variants: &[
        "Random",
        "Pitch Up",
        "Pitch Down",
        "Vel + Root",
        "Pitch Root",
        "Velocity",
        "Gate",
    ],
});

pub struct Params {
    mode: usize,
    midi_channel: MidiChannel,
    midi_in: MidiIn,
    midi_out: MidiOut,
    midi_note: MidiNote,
    span: i32,
    color: Color,
    range: Range,
    vpo: VoltPerOct,
    decay_mode: usize,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Append-only: old 9-param saves still load; Decay Mode defaults.
        if values.len() < 9 {
            return None;
        }
        Some(Self {
            mode: usize::from_value(values[0]).min(4),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_in: MidiIn::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
            midi_note: MidiNote::from_value(values[4]),
            span: i32::from_value(values[5]),
            color: Color::from_value(values[6]),
            range: Range::from_value(values[7]),
            vpo: VoltPerOct::from_value(values[8]),
            decay_mode: values
                .get(9)
                .map(|v| usize::from_value(*v).min(DECAY_MODE_COUNT - 1))
                .unwrap_or(DECAY_DEFAULT),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.span.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        vec.push(self.decay_mode.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    main_saved: u16,
    decay_saved: u16,
    bars_saved: u16,
    muted: bool,
    armed: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            main_saved: 2048,
            decay_saved: 0,
            bars_saved: 0,
            muted: false,
            armed: true,
        }
    }
}
impl AppStorage for Storage {}

#[derive(Clone, Copy, Default)]
struct NoteEvent {
    start: u16,
    dur: u16,
    note: u8,
    vel: u16,
    gen: u32,
    used: bool,
}

#[derive(Clone, Copy)]
struct LoopBuf {
    events: [NoteEvent; MAX_EVENTS],
    next_gen: u32,
    extent: u16,
    origin: u64,
}

impl Default for LoopBuf {
    fn default() -> Self {
        Self {
            events: [NoteEvent::default(); MAX_EVENTS],
            next_gen: 1,
            extent: 0,
            origin: u64::MAX,
        }
    }
}

impl LoopBuf {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn window_ticks(bars: u8) -> u16 {
        (bars.max(1) as u16)
            .saturating_mul(TICKS_PER_BAR)
            .min(MAX_BUFFER_TICKS)
    }

    fn play_pos(&self, tick: u64, bars: u8) -> Option<u16> {
        if self.origin == u64::MAX || self.extent == 0 {
            return None;
        }
        let wt = Self::window_ticks(bars) as u64;
        if wt == 0 {
            return None;
        }
        Some(((tick.saturating_sub(self.origin)) % wt) as u16)
    }

    fn ensure_origin(&mut self, tick: u64) {
        if self.origin == u64::MAX {
            self.origin = tick - (tick % TICKS_PER_16TH);
        }
    }

    fn add_or_refresh(&mut self, start: u16, dur: u16, note: u8, vel: u16, window: u16) {
        let start = start % window.max(1);
        let start_q = start / TICKS_PER_16TH as u16;

        for e in self.events.iter_mut() {
            if e.used && e.note == note && e.start / TICKS_PER_16TH as u16 == start_q {
                e.vel = vel;
                e.dur = dur.max(1);
                e.gen = self.next_gen;
                self.next_gen = self.next_gen.wrapping_add(1);
                let end = start.saturating_add(if dur == u16::MAX { 1 } else { dur });
                self.extent = self.extent.max(end.min(MAX_BUFFER_TICKS)).max(window.min(MAX_BUFFER_TICKS));
                return;
            }
        }

        let slot_i = if let Some(i) = self.events.iter().position(|e| !e.used) {
            i
        } else {
            let mut oldest_i = 0;
            let mut oldest_gen = u32::MAX;
            for (i, e) in self.events.iter().enumerate() {
                if e.gen < oldest_gen {
                    oldest_gen = e.gen;
                    oldest_i = i;
                }
            }
            oldest_i
        };

        let slot = &mut self.events[slot_i];
        slot.used = true;
        slot.start = start;
        slot.dur = dur.max(1);
        slot.note = note;
        slot.vel = vel;
        slot.gen = self.next_gen;
        self.next_gen = self.next_gen.wrapping_add(1);

        let end = start.saturating_add(if dur == u16::MAX { 1 } else { dur });
        self.extent = self
            .extent
            .max(end.min(MAX_BUFFER_TICKS))
            .max(window.min(MAX_BUFFER_TICKS));
    }

    fn finalize_open(&mut self, note: u8, end_pos: u16, window: u16) {
        let end_pos = end_pos % window.max(1);
        for e in self.events.iter_mut() {
            if e.used && e.note == note && e.dur == u16::MAX {
                let start = e.start;
                let dur = if end_pos >= start {
                    (end_pos - start).max(1)
                } else {
                    (window - start + end_pos).max(1)
                };
                e.dur = dur.min(window);
                return;
            }
        }
    }

    fn decay_all(
        &mut self,
        decay_fader: u16,
        mode: usize,
        base_note: u8,
        key: Key,
        tonic: Note,
        rng: &mut u32,
    ) {
        if decay_fader == 0 {
            return;
        }
        let target = snap_to_scale(base_note, key, tonic);
        let loss = (decay_fader as u32 * 1638) / 4095;
        let retain = 4095u32.saturating_sub(loss).max(2457);
        // At least ~12.5% of remaining offset per wrap; scales up with decay.
        let pitch_loss = (decay_fader as u32 * 2048 / 4095).max(512).max(loss);
        let steps = pitch_steps(decay_fader);

        for e in self.events.iter_mut() {
            if !e.used {
                continue;
            }
            match mode.min(DECAY_MODE_COUNT - 1) {
                DECAY_VEL => {
                    apply_velocity_decay(e, retain);
                }
                DECAY_PITCH_ROOT => {
                    pull_pitch_to_root(e, target, pitch_loss, key, tonic);
                }
                DECAY_VEL_ROOT => {
                    apply_velocity_decay(e, retain);
                    if e.used {
                        pull_pitch_to_root(e, target, pitch_loss, key, tonic);
                    }
                }
                DECAY_PITCH_UP => {
                    shift_pitch_steps(e, steps, true, key, tonic);
                }
                DECAY_PITCH_DOWN => {
                    shift_pitch_steps(e, steps, false, key, tonic);
                }
                DECAY_GATE => {
                    let min_dur = TICKS_PER_16TH as u16;
                    if e.dur != u16::MAX {
                        let nd = ((e.dur as u32 * retain) / 4095) as u16;
                        e.dur = nd.max(min_dur).min(e.dur);
                    }
                }
                DECAY_RANDOM => {
                    // Random: scale pitch jitter + velocity only goes down.
                    let max_steps = steps.max(1);
                    let n = rand_below(rng, max_steps + 1);
                    if n > 0 {
                        let up = rand_below(rng, 2) == 1;
                        shift_pitch_steps(e, n, up, key, tonic);
                    }
                    let actual_loss = rand_below(rng, loss + 1);
                    let rnd_retain = 4095u32.saturating_sub(actual_loss).max(2457);
                    apply_velocity_decay(e, rnd_retain);
                }
                _ => {}
            }
        }
        if self.events.iter().all(|e| !e.used) {
            self.extent = 0;
            self.origin = u64::MAX;
        }
    }

    fn set_window_bars(&mut self, old_bars: u8, new_bars: u8) {
        let old_w = Self::window_ticks(old_bars);
        let new_w = Self::window_ticks(new_bars);
        if new_w <= old_w {
            return;
        }
        // Virgin extend: tile current audible window into new space.
        if self.extent > 0 && self.extent <= old_w {
            let tile_end = old_w.min(self.extent);
            if tile_end == 0 {
                return;
            }
            let mut extras: Vec<NoteEvent, MAX_EVENTS> = Vec::new();
            for e in self.events.iter() {
                if !e.used || e.start >= tile_end || e.dur == u16::MAX {
                    continue;
                }
                let mut pos = old_w;
                while pos < new_w {
                    let mut ne = *e;
                    ne.start = pos.saturating_add(e.start);
                    if ne.start >= new_w {
                        break;
                    }
                    if ne.start.saturating_add(ne.dur) > new_w {
                        ne.dur = new_w.saturating_sub(ne.start);
                    }
                    ne.gen = self.next_gen;
                    self.next_gen = self.next_gen.wrapping_add(1);
                    let _ = extras.push(ne);
                    let next = pos.saturating_add(tile_end);
                    if next <= pos {
                        break;
                    }
                    pos = next;
                }
            }
            for ne in extras {
                if let Some(slot) = self.events.iter_mut().find(|e| !e.used) {
                    *slot = ne;
                    slot.used = true;
                }
            }
            self.extent = new_w;
        }
        // else: re-extend into preserved region — reveal only.
    }
}

fn note_u8(n: MidiNote) -> u8 {
    u7::from(n).as_int()
}

fn bars_from_fader(v: u16) -> u8 {
    ((v as u32 * (MAX_BARS as u32 - 1)) / 4095) as u8 + 1
}

fn fader_from_bars(bars: u8) -> u16 {
    let b = bars.clamp(1, MAX_BARS) as u32 - 1;
    ((b * 4095) / (MAX_BARS as u32 - 1)) as u16
}

fn decay_mode_from_fader(v: u16) -> usize {
    ((v as u32 * DECAY_MODE_COUNT as u32) / 4096).min(DECAY_MODE_COUNT as u32 - 1) as usize
}

fn fader_from_decay_mode(mode: usize) -> u16 {
    let i = mode.min(DECAY_MODE_COUNT - 1) as u32;
    ((i * 2 + 1) * 4096 / (DECAY_MODE_COUNT as u32 * 2)) as u16
}

fn decay_mode_color(mode: usize) -> Color {
    const COLORS: [Color; DECAY_MODE_COUNT] = [
        Color::Rose,   // Random
        Color::Orange, // Pitch Up
        Color::Yellow, // Pitch Down
        Color::Lime,   // Vel + Root
        Color::Cyan,   // Pitch Root
        Color::Blue,   // Velocity
        Color::Violet, // Gate
    ];
    COLORS[mode.min(DECAY_MODE_COUNT - 1)]
}

fn pitch_steps(decay_fader: u16) -> u32 {
    ((decay_fader as u32 * 4) / 4095).max(1)
}

fn xorshift32(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

fn rand_below(state: &mut u32, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    if *state == 0 {
        *state = 0xA5A5_5A5A;
    }
    xorshift32(state) % max
}

fn apply_velocity_decay(e: &mut NoteEvent, retain: u32) {
    let nv = ((e.vel as u32 * retain) / 4095) as u16;
    if nv <= KILL_FLOOR {
        e.used = false;
        e.vel = 0;
    } else {
        e.vel = nv;
    }
}

fn pull_pitch_to_root(e: &mut NoteEvent, target: u8, pitch_loss: u32, key: Key, tonic: Note) {
    let pitch_offset = e.note as i32 - target as i32;
    if pitch_offset == 0 {
        return;
    }
    let dist = pitch_offset.unsigned_abs();
    // Always move ≥1 semitone toward target when decay > 0.
    let pull = ((dist * pitch_loss) / 4095).max(1) as i32;
    let new_offset = if pitch_offset > 0 {
        (pitch_offset - pull).max(0)
    } else {
        (pitch_offset + pull).min(0)
    };
    let chromatic = (target as i32 + new_offset).clamp(0, 127) as u8;
    e.note = snap_to_scale_toward(chromatic, key, tonic, Some(target));
}

fn next_scale_degree(note: u8, up: bool, key: Key, tonic: Note) -> u8 {
    if matches!(key, Key::Chromatic | Key::Off) {
        return if up {
            note.saturating_add(1).min(127)
        } else {
            note.saturating_sub(1)
        };
    }
    for d in 1u8..=12 {
        let candidate = if up {
            if note > 127 - d {
                return note;
            }
            note + d
        } else {
            match note.checked_sub(d) {
                Some(n) => n,
                None => return note,
            }
        };
        if pc_in_scale(candidate % 12, key, tonic) {
            return candidate;
        }
    }
    note
}

fn shift_pitch_steps(e: &mut NoteEvent, steps: u32, up: bool, key: Key, tonic: Note) {
    let mut n = e.note;
    for _ in 0..steps {
        let next = next_scale_degree(n, up, key, tonic);
        if next == n {
            break;
        }
        n = next;
    }
    e.note = snap_to_scale(n, key, tonic);
}

fn quantize_16th(pos: u16) -> u16 {
    let q = TICKS_PER_16TH as u16;
    (pos / q) * q
}

fn mode_color(mode: usize) -> Color {
    const MODES: [Color; 5] = [
        Color::Cyan,
        Color::Orange,
        Color::Green,
        Color::Blue,
        Color::Pink,
    ];
    MODES[mode.min(4)]
}

/// Bipolar fader offset: bottom = −span/2, centre = 0, top = +span/2.
fn fader_semitones(fader: u16, span: i32) -> i32 {
    ((fader as i32 - 2048) * span) / 4096
}

fn pitch_from_fader(fader: u16, base: MidiNote, span: i32) -> u8 {
    (note_u8(base) as i32 + fader_semitones(fader, span)).clamp(0, 127) as u8
}

fn pc_in_scale(pc: u8, key: Key, tonic: Note) -> bool {
    let mask = key.as_u16_key();
    let tonic_pc = (tonic as u8) % 12;
    let rel = (pc + 12 - tonic_pc) % 12;
    (mask >> (11 - rel as u16)) & 1 != 0
}

/// Snap MIDI note to nearest degree of the global key/tonic. Chromatic / Off = passthrough.
fn snap_to_scale(note: u8, key: Key, tonic: Note) -> u8 {
    snap_to_scale_toward(note, key, tonic, None)
}

/// Next MIDI note one scale degree (or semitone) toward `target`.
fn step_toward(cur: u8, target: u8, key: Key, tonic: Note) -> u8 {
    if cur == target {
        return cur;
    }
    let dir: i16 = if target > cur { 1 } else { -1 };
    let chromatic = matches!(key, Key::Chromatic | Key::Off);
    let mut n = cur as i16;
    for _ in 0..12 {
        n += dir;
        if !(0..=127).contains(&n) {
            return cur;
        }
        let cand = n as u8;
        if cand == target || chromatic || pc_in_scale(cand % 12, key, tonic) {
            return cand;
        }
    }
    cur
}

fn snap_to_scale_toward(note: u8, key: Key, tonic: Note, toward: Option<u8>) -> u8 {
    if matches!(key, Key::Chromatic | Key::Off) {
        return note;
    }
    if pc_in_scale(note % 12, key, tonic) {
        return note;
    }
    for d in 1u8..=6 {
        let down = note.checked_sub(d);
        let up = (note <= 127 - d).then_some(note + d);
        let down_ok = down.is_some_and(|n| pc_in_scale(n % 12, key, tonic));
        let up_ok = up.is_some_and(|n| pc_in_scale(n % 12, key, tonic));
        match (down_ok, up_ok, toward) {
            (true, true, Some(t)) => {
                let dn = down.unwrap();
                let un = up.unwrap();
                let dd = (dn as i16 - t as i16).unsigned_abs();
                let du = (un as i16 - t as i16).unsigned_abs();
                return if du < dd { un } else { dn };
            }
            (true, _, _) => return down.unwrap(),
            (_, true, _) => return up.unwrap(),
            _ => {}
        }
    }
    note
}

/// HSV with explicit S/V (0..=255). Hue in degrees 0..360.
fn hsv_to_rgb(hue: u16, sat: u8, val: u8) -> (u8, u8, u8) {
    if sat == 0 {
        return (val, val, val);
    }
    let sector = (hue % 360) / 60;
    let f = (hue % 60) as u32;
    let p = (val as u32 * (255 - sat as u32) / 255) as u8;
    let q = (val as u32 * (255 - (sat as u32 * f) / 60) / 255) as u8;
    let t = (val as u32 * (255 - (sat as u32 * (60 - f)) / 60) / 255) as u8;
    match sector {
        0 => (val, t, p),
        1 => (q, val, p),
        2 => (p, val, t),
        3 => (p, q, val),
        4 => (t, p, val),
        _ => (val, p, q),
    }
}

fn fader_hsv_color(hue: u16, _fader: u16) -> Color {
    // Full sat/value — level is shown via Top/Bottom brightness, not washed RGB.
    let (r, g, b) = hsv_to_rgb(hue, 255, 255);
    Color::Custom(r, g, b)
}

fn pitch_fader_color(fader: u16) -> Color {
    // Spectrum with pitch: low → red, high → violet.
    let hue = (fader.min(4095) as u32 * 270 / 4095) as u16;
    fader_hsv_color(hue, fader)
}

fn decay_fader_color(fader: u16) -> Color {
    // Slow = cool violet; fast kill = hot red.
    let t = fader.min(4095) as u32;
    let hue = (300u32 - (t * 300) / 4095) as u16;
    fader_hsv_color(hue, fader)
}

fn bars_fader_color(bars: u8, fader: u16) -> Color {
    // 1 bar = amber, 8 bars = cyan.
    let hue = (40u16).saturating_add((bars.saturating_sub(1) as u16) * 20);
    fader_hsv_color(hue.min(180), fader)
}

fn paint_fader_meters<const N: usize>(
    leds: &crate::app::Leds<N>,
    color: Color,
    fader: u16,
    button_bright: u8,
) {
    let led = split_unsigned_value(fader);
    leds.set(0, Led::Top, color, Brightness::Custom(led[0].max(12)));
    leds.set(0, Led::Bottom, color, Brightness::Custom(led[1].max(12)));
    leds.set(
        0,
        Led::Button,
        color,
        Brightness::Custom(button_bright.max(24)),
    );
}

#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            mode: 4,
            midi_channel: MidiChannel::default(),
            midi_in: MidiIn::default(),
            midi_out: MidiOut::default(),
            midi_note: MidiNote::from(48),
            span: 24,
            color: Color::Violet,
            range: Range::_0_10V,
            vpo: VoltPerOct::Standard,
            decay_mode: DECAY_DEFAULT,
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

    let (mode, midi_chan, midi_in_src, midi_out_dst, base_note, span, _base_color, range, vpo, decay_mode) =
        params.query(|p| {
            (
                p.mode.min(4),
                p.midi_channel,
                p.midi_in,
                p.midi_out,
                p.midi_note,
                p.span,
                p.color,
                p.range,
                p.vpo,
                p.decay_mode.min(DECAY_MODE_COUNT - 1),
            )
        });

    let led_color = mode_color(mode);
    let needs_cv_in = mode == 0 || mode == 1;
    let needs_cv_out = mode == 3 || mode == 4;
    let uses_button_gate = mode == 0 || mode == 4;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let quantizer = app.use_quantizer(range, vpo, false);
    let midi = app.use_midi_output(midi_out_dst, midi_chan, false);

    let in_jack: Option<InJack> = if needs_cv_in {
        Some(app.make_in_jack(0, range).await)
    } else {
        None
    };
    let out_jack: Option<OutJack> = if needs_cv_out {
        Some(app.make_out_jack(0, range).await)
    } else {
        None
    };
    if let Some(ref j) = out_jack {
        j.set_value(0);
    }

    let buf = app.make_global(LoopBuf::default());
    let bars_glob = app.make_global(bars_from_fader(storage.query(|s| s.bars_saved)));
    let decay_glob = app.make_global(storage.query(|s| s.decay_saved));
    let decay_mode_glob = app.make_global(decay_mode);
    let main_glob = app.make_global(storage.query(|s| s.main_saved));
    let muted = app.make_global(storage.query(|s| s.muted));
    let armed = app.make_global(storage.query(|s| s.armed));
    let fader_moved_while_held = app.make_global(false);
    let button_down_fader = app.make_global(0u16);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let flash_until = app.make_global(0u64);
    let erase_flash = app.make_global(false);
    let bars_flash = app.make_global(false);
    let live_note = app.make_global(None::<u8>);
    let rec_open = app.make_global(false);
    let voices = app.make_global([(0u8, 0u16, 0u32); POLY]);
    let glob_ticks = app.make_global(0u64);
    let glob_clock_reset = app.make_global(false);
    let glob_clock_stop = app.make_global(false);
    let glob_clock_start = app.make_global(false);
    let rng_glob = app.make_global((0x00C0_FFEEu32).wrapping_mul(1));

    if muted.get() {
        leds.set(0, Led::Button, Color::Red, Brightness::Low);
    } else {
        let bright = if armed.get() {
            Brightness::High
        } else {
            Brightness::Low
        };
        leds.set(0, Led::Button, led_color, bright);
    }

    let clock_drain = async {
        let mut clock = app.use_clock();
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick(tick) => {
                    glob_ticks.set(tick);
                }
                ClockEvent::Reset => {
                    glob_clock_reset.set(true);
                }
                ClockEvent::Stop => {
                    glob_clock_stop.set(true);
                }
                ClockEvent::Start => {
                    glob_clock_start.set(true);
                }
            }
        }
    };

    let clock_processor = async {
        let mut prev_pos: Option<u16> = None;
        let mut last_processed_tick: u64 = 0;

        loop {
            app.delay_millis(1).await;

            if glob_clock_stop.get() {
                glob_clock_stop.set(false);
                all_notes_off(&midi, &voices, &live_note, out_jack.as_ref()).await;
                prev_pos = None;
            }

            if glob_clock_reset.get() {
                glob_clock_reset.set(false);
                all_notes_off(&midi, &voices, &live_note, out_jack.as_ref()).await;
                let mut b = buf.get();
                if b.origin != u64::MAX {
                    b.origin = glob_ticks.get();
                    buf.set(b);
                }
                prev_pos = None;
            }

            if glob_clock_start.get() {
                glob_clock_start.set(false);
                // Tick counter restarts at 0 — re-anchor origin or play_pos
                // saturates to 0 forever and the loop never advances.
                let mut b = buf.get();
                if b.origin != u64::MAX {
                    b.origin = glob_ticks.get();
                    buf.set(b);
                }
                prev_pos = None;
            }

            let tick = glob_ticks.get();
            if tick == last_processed_tick {
                continue;
            }
            last_processed_tick = tick;

            let bars = bars_glob.get();
            let mut b = buf.get();
            let window = LoopBuf::window_ticks(bars);

            if let Some(pos) = b.play_pos(tick, bars) {
                if let Some(pp) = prev_pos {
                    if pos < pp && !muted.get() {
                        let (key, tonic) = quantizer.get_scale().await;
                        let mut rng = rng_glob.get();
                        b.decay_all(
                            decay_glob.get(),
                            decay_mode_glob.get(),
                            note_u8(base_note),
                            key,
                            tonic,
                            &mut rng,
                        );
                        rng_glob.set(rng);
                        buf.set(b);
                        b = buf.get();
                    }
                }
                let stepped_from = prev_pos;
                prev_pos = Some(pos);

                if !muted.get() && b.extent > 0 {
                    for e in b.events.iter() {
                        if !e.used || e.dur == u16::MAX {
                            continue;
                        }
                        if crossed(stepped_from, pos, e.start) {
                            alloc_voice(&midi, &voices, e.note, e.vel, e.gen, e, window)
                                .await;
                            flash_until.set(tick.wrapping_add(CAPTURE_FLASH_TICKS));
                        }
                    }

                    let mut vs = voices.get();
                    for v in vs.iter_mut() {
                        if v.0 != 0 && crossed(stepped_from, pos, v.1) {
                            midi.send_note_off(MidiNote::from(v.0)).await;
                            *v = (0, 0, 0);
                        }
                    }
                    voices.set(vs);

                    if let Some(ref j) = out_jack {
                        update_cv_out(
                            j,
                            &quantizer,
                            &live_note,
                            &voices,
                            base_note,
                            range,
                            vpo,
                        )
                        .await;
                    }
                }
            } else {
                prev_pos = None;
            }
        }
    };

    let fader_task = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change().await;
            let layer = glob_latch_layer.get();
            let val = faders.get_value();

            if buttons.is_button_pressed(0) {
                let start = button_down_fader.get();
                if start.abs_diff(val) > FADER_MOVE_THRESH {
                    fader_moved_while_held.set(true);
                }
            }

            let target = match layer {
                LatchLayer::Main => storage.query(|s| s.main_saved),
                LatchLayer::Alt => storage.query(|s| s.decay_saved),
                LatchLayer::Third => fader_from_decay_mode(decay_mode_glob.get()),
            };

            if let Some(new_value) = latch.update(val, layer, target) {
                match layer {
                    LatchLayer::Alt => {
                        decay_glob.set(new_value);
                        storage.modify_and_save(|s| s.decay_saved = new_value);
                        paint_fader_meters(
                            &leds,
                            decay_fader_color(new_value),
                            new_value,
                            (new_value / 16) as u8,
                        );
                    }
                    LatchLayer::Third => {
                        let mode = decay_mode_from_fader(new_value);
                        if mode != decay_mode_glob.get() {
                            decay_mode_glob.set(mode);
                        }
                        let center = fader_from_decay_mode(mode);
                        paint_fader_meters(
                            &leds,
                            decay_mode_color(mode),
                            center,
                            255,
                        );
                    }
                    LatchLayer::Main => {
                        main_glob.set(new_value);
                        storage.modify_and_save(|s| s.main_saved = new_value);
                        let pitch_c = pitch_fader_color(new_value);
                        let meter = split_unsigned_value(new_value);
                        leds.set(0, Led::Top, pitch_c, Brightness::Custom(meter[0].max(8)));
                        leds.set(0, Led::Bottom, pitch_c, Brightness::Custom(meter[1].max(8)));
                    }
                }
            }
        }
    };

    let controls = async {
        loop {
            // Use down-event shift flag — polling after await misses Shift often.
            let shift = buttons.wait_for_down(0).await;
            fader_moved_while_held.set(false);
            button_down_fader.set(faders.get_value());

            if shift {
                // Shift+Short = cycle bars; Shift+Long = mute, or erase if muted.
                match select(buttons.wait_for_up(0), app.delay_millis(SHIFT_LONG_MS)).await {
                    Either::First(_) => {
                        let old = bars_glob.get();
                        let new_bars = if old >= MAX_BARS { 1 } else { old + 1 };
                        let mut b = buf.get();
                        b.set_window_bars(old, new_bars);
                        buf.set(b);
                        bars_glob.set(new_bars);
                        storage.modify_and_save(|s| s.bars_saved = fader_from_bars(new_bars));
                        bars_flash.set(true);
                    }
                    Either::Second(_) => {
                        if muted.get() {
                            let mut b = buf.get();
                            b.clear();
                            buf.set(b);
                            all_notes_off(&midi, &voices, &live_note, out_jack.as_ref()).await;
                            erase_flash.set(true);
                        } else {
                            muted.set(true);
                            storage.modify_and_save(|s| s.muted = true);
                            all_notes_off(&midi, &voices, &live_note, out_jack.as_ref()).await;
                        }
                        buttons.wait_for_up(0).await;
                    }
                }
                continue;
            }

            if muted.get() {
                // Short = unmute (and re-arm so a prior Long doesn't leave us silent).
                // Long while muted = ignore (length is Shift+Short).
                match select(buttons.wait_for_up(0), app.delay_millis(SHIFT_LONG_MS)).await {
                    Either::First(_) => {
                        if !fader_moved_while_held.get() {
                            muted.set(false);
                            armed.set(true);
                            storage.modify_and_save(|s| {
                                s.muted = false;
                                s.armed = true;
                            });
                        }
                    }
                    Either::Second(_) => {
                        buttons.wait_for_up(0).await;
                    }
                }
                continue;
            }

            if uses_button_gate {
                let note = resolve_pitch(
                    mode,
                    &quantizer,
                    in_jack.as_ref(),
                    main_glob.get(),
                    base_note,
                    span,
                )
                .await;
                start_note(
                    &midi,
                    &buf,
                    &bars_glob,
                    &armed,
                    &live_note,
                    &rec_open,
                    &flash_until,
                    &glob_ticks,
                    note,
                    4095,
                    out_jack.as_ref(),
                    range,
                    vpo,
                    &quantizer,
                    needs_cv_out,
                )
                .await;

                // Hold+Fader: step through scale degrees toward the fader (Vamp-style).
                let mut glide_frames_left: u16 = 0;
                loop {
                    match select(buttons.wait_for_up(0), app.delay_millis(1)).await {
                        Either::First(_) => {
                            note_off(
                                &midi,
                                &buf,
                                &bars_glob,
                                &live_note,
                                &rec_open,
                                &armed,
                                &glob_ticks,
                                out_jack.as_ref(),
                            )
                            .await;
                            break;
                        }
                        Either::Second(_) => {
                            // Hold+Fader gesture: a plain hold must sustain. The
                            // target includes CV in, so chasing it unconditionally
                            // retriggers the held note whenever the input moves —
                            // or merely jitters across a quantizer step.
                            if !fader_moved_while_held.get() {
                                glide_frames_left = 0;
                                continue;
                            }
                            let target = resolve_pitch(
                                mode,
                                &quantizer,
                                in_jack.as_ref(),
                                main_glob.get(),
                                base_note,
                                span,
                            )
                            .await;
                            let Some(cur) = live_note.get() else {
                                continue;
                            };
                            if cur == target {
                                glide_frames_left = 0;
                                continue;
                            }
                            if glide_frames_left == 0 {
                                let (key, tonic) = quantizer.get_scale().await;
                                let next = step_toward(cur, target, key, tonic);
                                if next != cur {
                                    retune_live(
                                        &midi,
                                        &buf,
                                        &bars_glob,
                                        &armed,
                                        &live_note,
                                        &rec_open,
                                        &flash_until,
                                        &glob_ticks,
                                        next,
                                        4095,
                                        out_jack.as_ref(),
                                        range,
                                        vpo,
                                        &quantizer,
                                        needs_cv_out,
                                    )
                                    .await;
                                }
                                glide_frames_left = if next == target { 0 } else { GLISS_FRAMES };
                            } else {
                                glide_frames_left = glide_frames_left.saturating_sub(1);
                            }
                        }
                    }
                }
                continue;
            }

            // Non-gate modes: Long (no fader move) = mute.
            // Mute is Shift+Long in gate modes — button is the note gate.
            match select(buttons.wait_for_up(0), app.delay_millis(SHIFT_LONG_MS)).await {
                Either::First(_) => {}
                Either::Second(_) => {
                    if !fader_moved_while_held.get() {
                        if let Some(n) = live_note.get() {
                            midi.send_note_off(MidiNote::from(n)).await;
                            live_note.set(None);
                            rec_open.set(false);
                        }
                        muted.set(true);
                        storage.modify_and_save(|s| s.muted = true);
                        all_notes_off(&midi, &voices, &live_note, out_jack.as_ref()).await;
                    }
                    buttons.wait_for_up(0).await;
                }
            }
        }
    };

    let input_task = async {
        match mode {
            1 => {
                let jack = in_jack.as_ref().unwrap();
                let mut old = jack.get_value();
                loop {
                    app.delay_millis(1).await;
                    let v = jack.get_value();
                    if muted.get() {
                        old = v;
                        continue;
                    }
                    if v >= GATE_THRESH && old < GATE_THRESH {
                        let (key, tonic) = quantizer.get_scale().await;
                        let note = snap_to_scale(
                            pitch_from_fader(main_glob.get(), base_note, span),
                            key,
                            tonic,
                        );
                        start_note(
                            &midi,
                            &buf,
                            &bars_glob,
                            &armed,
                            &live_note,
                            &rec_open,
                            &flash_until,
                            &glob_ticks,
                            note,
                            4095,
                            out_jack.as_ref(),
                            range,
                            vpo,
                            &quantizer,
                            false,
                        )
                        .await;
                    } else if v < GATE_THRESH && old >= GATE_THRESH {
                        note_off(
                            &midi,
                            &buf,
                            &bars_glob,
                            &live_note,
                            &rec_open,
                            &armed,
                            &glob_ticks,
                            out_jack.as_ref(),
                        )
                        .await;
                    }
                    old = v;
                }
            }
            2 | 3 => {
                let mut midi_in = app.use_midi_input(midi_in_src, midi_chan);
                loop {
                    match midi_in.wait_for_event().await {
                        AppMidiEvent::Message(MidiMessage::NoteOn { key, vel }) => {
                            if muted.get() {
                                continue;
                            }
                            let (scale_key, tonic) = quantizer.get_scale().await;
                            let n = snap_to_scale(u8::from(key), scale_key, tonic);
                            if u8::from(vel) == 0 {
                                end_specific(
                                    &midi,
                                    &buf,
                                    &bars_glob,
                                    &live_note,
                                    &rec_open,
                                    &armed,
                                    &glob_ticks,
                                    n,
                                    out_jack.as_ref(),
                                )
                                .await;
                            } else {
                                let v12 = ((u8::from(vel) as u32 * 4095) / 127) as u16;
                                start_note(
                                    &midi,
                                    &buf,
                                    &bars_glob,
                                    &armed,
                                    &live_note,
                                    &rec_open,
                                    &flash_until,
                                    &glob_ticks,
                                    n,
                                    v12.max(KILL_FLOOR + 1),
                                    out_jack.as_ref(),
                                    range,
                                    vpo,
                                    &quantizer,
                                    mode == 3,
                                )
                                .await;
                            }
                        }
                        AppMidiEvent::Message(MidiMessage::NoteOff { key, .. }) => {
                            let (scale_key, tonic) = quantizer.get_scale().await;
                            let n = snap_to_scale(u8::from(key), scale_key, tonic);
                            end_specific(
                                &midi,
                                &buf,
                                &bars_glob,
                                &live_note,
                                &rec_open,
                                &armed,
                                &glob_ticks,
                                n,
                                out_jack.as_ref(),
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
            }
            _ => loop {
                app.delay_millis(1000).await;
            },
        }
    };

    let layer_task = async {
        // LEDs live here (1 ms), not on clock ticks — mute/arm/erase must
        // show even when the internal/external clock is stopped.
        let mut blink_on = true;
        let mut last_blink = Instant::now();
        let mut erase_hold_ms = 0u16;
        let mut bars_hold_ms = 0u16;
        loop {
            app.delay_millis(1).await;
            // Shift alone = Alt (strength). Btn (no shift) = Third (decay mode);
            // in gate modes Third only while muted so hold-to-play stays pitch.
            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed()
                && buttons.is_button_pressed(0)
                && (muted.get() || !uses_button_gate)
            {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(layer);

            if last_blink.elapsed().as_millis() >= ARM_BLINK_MS {
                blink_on = !blink_on;
                last_blink = Instant::now();
            }

            if erase_flash.get() {
                erase_flash.set(false);
                erase_hold_ms = 120;
            }
            if bars_flash.get() {
                bars_flash.set(false);
                bars_hold_ms = 220;
            }
            if erase_hold_ms > 0 {
                erase_hold_ms = erase_hold_ms.saturating_sub(1);
                leds.set(0, Led::Button, Color::White, Brightness::High);
                continue;
            }
            if bars_hold_ms > 0 {
                bars_hold_ms = bars_hold_ms.saturating_sub(1);
                let bars = bars_glob.get();
                let v = fader_from_bars(bars);
                // Solid meters + button strobe so length is obvious without a loop.
                paint_fader_meters(
                    &leds,
                    bars_fader_color(bars, v),
                    v,
                    255,
                );
                if (bars_hold_ms / 40).is_multiple_of(2) {
                    leds.set(0, Led::Button, bars_fader_color(bars, v), Brightness::High);
                } else {
                    leds.unset(0, Led::Button);
                }
                continue;
            }

            match layer {
                LatchLayer::Alt => {
                    let v = decay_glob.get();
                    paint_fader_meters(&leds, decay_fader_color(v), v, (v / 16) as u8);
                }
                LatchLayer::Third => {
                    let mode = decay_mode_glob.get();
                    let v = fader_from_decay_mode(mode);
                    paint_fader_meters(&leds, decay_mode_color(mode), v, 255);
                }
                LatchLayer::Main => {
                    if muted.get() {
                        leds.set(0, Led::Button, Color::Red, Brightness::Low);
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    } else {
                        let main = main_glob.get();
                        let pitch_c = pitch_fader_color(main);
                        let meter = split_unsigned_value(main);
                        leds.set(0, Led::Top, pitch_c, Brightness::Custom(meter[0].max(8)));
                        leds.set(0, Led::Bottom, pitch_c, Brightness::Custom(meter[1].max(8)));
                        let bright = if armed.get() {
                            if blink_on {
                                Brightness::High
                            } else {
                                Brightness::Mid
                            }
                        } else {
                            Brightness::Low
                        };
                        let tick = glob_ticks.get();
                        let capturing = flash_until.get() > tick;
                        let button_bright = if capturing {
                            Brightness::Custom(
                                ((u8::from(bright) as u16 * CAPTURE_DIM_REMAIN_PCT) / 100).max(1)
                                    as u8,
                            )
                        } else {
                            bright
                        };
                        leds.set(0, Led::Button, led_color, button_bright);
                        if capturing {
                            leds.set(0, Led::Top, Color::White, Brightness::High);
                        }
                    }
                }
            }
        }
    };

    let scene_task = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    muted.set(storage.query(|s| s.muted));
                    armed.set(storage.query(|s| s.armed));
                    decay_glob.set(storage.query(|s| s.decay_saved));
                    main_glob.set(storage.query(|s| s.main_saved));
                    bars_glob.set(bars_from_fader(storage.query(|s| s.bars_saved)));
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join5(
        join(clock_drain, clock_processor),
        fader_task,
        controls,
        join(input_task, layer_task),
        scene_task,
    )
    .await;
}

async fn resolve_pitch(
    mode: usize,
    quantizer: &Quantizer,
    in_jack: Option<&InJack>,
    main: u16,
    base_note: MidiNote,
    span: i32,
) -> u8 {
    let raw = if mode == 0 {
        if let Some(j) = in_jack {
            // CV pitch and fader stack. mode 0 always has an in jack, so reading
            // CV alone left the fader (and Span) dead whenever nothing was
            // patched: an open input sits near 0 V, pinning every note to Base.
            // Unpatched the fader keeps its documented span; with a cable it
            // transposes, and its centre detent is a 0-semitone no-op.
            let pitch = quantizer.get_quantized_note(j.get_value()).await;
            (note_u8(base_note) as i32
                + note_u8(pitch.as_midi()) as i32
                + fader_semitones(main, span))
            .clamp(0, 127) as u8
        } else {
            pitch_from_fader(main, base_note, span)
        }
    } else {
        pitch_from_fader(main, base_note, span)
    };
    let (key, tonic) = quantizer.get_scale().await;
    snap_to_scale(raw, key, tonic)
}

async fn all_notes_off(
    midi: &MidiOutput,
    voices: &Global<[(u8, u16, u32); POLY]>,
    live_note: &Global<Option<u8>>,
    out_jack: Option<&OutJack>,
) {
    let vs = voices.get();
    for (n, _, _) in vs {
        if n > 0 {
            midi.send_note_off(MidiNote::from(n)).await;
        }
    }
    voices.set([(0, 0, 0); POLY]);
    if let Some(n) = live_note.get() {
        midi.send_note_off(MidiNote::from(n)).await;
        live_note.set(None);
    }
    if let Some(j) = out_jack {
        j.set_value(0);
    }
}

/// Did the loop just advance across `target`?
///
/// Clock ticks can be skipped — the gatekeeper publishes immediately, so a
/// subscriber that falls behind drops ticks rather than stalling the device
/// clock — and `pos` then jumps over the position it was supposed to land on.
/// An `== pos` test misses that and leaves the note sounding until its voice
/// slot is stolen, so compare against the whole span since the last tick.
fn crossed(prev: Option<u16>, pos: u16, target: u16) -> bool {
    match prev {
        Some(pp) if pp < pos => target > pp && target <= pos,
        // Wrapped around the loop window.
        Some(pp) if pp > pos => target > pp || target <= pos,
        _ => target == pos,
    }
}

async fn alloc_voice(
    midi: &MidiOutput,
    voices: &Global<[(u8, u16, u32); POLY]>,
    note: u8,
    vel: u16,
    gen: u32,
    e: &NoteEvent,
    window: u16,
) {
    let end = e.start.saturating_add(e.dur) % window.max(1);
    let mut vs = voices.get();
    for v in vs.iter_mut() {
        if v.0 == note {
            midi.send_note_off(MidiNote::from(note)).await;
            *v = (note, end, gen);
            midi.send_note_on(MidiNote::from(note), vel).await;
            voices.set(vs);
            return;
        }
    }
    if let Some(slot) = vs.iter_mut().find(|v| v.0 == 0) {
        *slot = (note, end, gen);
        midi.send_note_on(MidiNote::from(note), vel).await;
        voices.set(vs);
        return;
    }
    let mut oi = 0;
    let mut og = u32::MAX;
    for (i, v) in vs.iter().enumerate() {
        if v.2 < og {
            og = v.2;
            oi = i;
        }
    }
    midi.send_note_off(MidiNote::from(vs[oi].0)).await;
    vs[oi] = (note, end, gen);
    midi.send_note_on(MidiNote::from(note), vel).await;
    voices.set(vs);
}

async fn update_cv_out(
    j: &OutJack,
    quantizer: &Quantizer,
    live_note: &Global<Option<u8>>,
    voices: &Global<[(u8, u16, u32); POLY]>,
    base_note: MidiNote,
    range: Range,
    vpo: VoltPerOct,
) {
    let vs = voices.get();
    let note = live_note
        .get()
        .or_else(|| vs.iter().find(|v| v.0 != 0).map(|v| v.0));
    if let Some(n) = note {
        let base = note_u8(base_note);
        // Base at mid-travel; ±60 semitones map to the full CV range.
        let rel = (n as i32 - base as i32).clamp(-60, 60);
        let counts = (((rel + 60) as u32 * 4095) / 120) as u16;
        let pitch = quantizer.get_quantized_note(counts).await;
        j.set_value(pitch.as_counts(range, vpo));
    } else {
        j.set_value(0);
    }
}

#[allow(clippy::too_many_arguments)]
async fn start_note(
    midi: &MidiOutput,
    buf: &Global<LoopBuf>,
    bars_glob: &Global<u8>,
    armed: &Global<bool>,
    live_note: &Global<Option<u8>>,
    rec_open: &Global<bool>,
    flash_until: &Global<u64>,
    glob_ticks: &Global<u64>,
    note: u8,
    vel: u16,
    out_jack: Option<&OutJack>,
    range: Range,
    vpo: VoltPerOct,
    quantizer: &Quantizer,
    cv_out: bool,
) {
    if let Some(old) = live_note.get() {
        midi.send_note_off(MidiNote::from(old)).await;
    }
    live_note.set(Some(note));
    midi.send_note_on(MidiNote::from(note), vel).await;

    if cv_out {
        if let Some(j) = out_jack {
            let counts = ((note as u32 * 4095) / 127) as u16;
            let pitch = quantizer.get_quantized_note(counts).await;
            j.set_value(pitch.as_counts(range, vpo));
        }
    }

    if armed.get() {
        let tick = glob_ticks.get();
        let bars = bars_glob.get();
        let window = LoopBuf::window_ticks(bars).max(1);
        let mut b = buf.get();
        b.ensure_origin(tick);
        // Don't use play_pos here — it returns None while extent==0 (first hit).
        let pos = ((tick.saturating_sub(b.origin)) % window as u64) as u16;
        let qpos = quantize_16th(pos) % window;
        b.add_or_refresh(qpos, u16::MAX, note, vel, window);
        buf.set(b);
        rec_open.set(true);
        flash_until.set(tick.wrapping_add(CAPTURE_FLASH_TICKS));
    }
}

/// Glissando step: close the sounding note (and its open record) then attack the next degree.
#[allow(clippy::too_many_arguments)]
async fn retune_live(
    midi: &MidiOutput,
    buf: &Global<LoopBuf>,
    bars_glob: &Global<u8>,
    armed: &Global<bool>,
    live_note: &Global<Option<u8>>,
    rec_open: &Global<bool>,
    flash_until: &Global<u64>,
    glob_ticks: &Global<u64>,
    note: u8,
    vel: u16,
    out_jack: Option<&OutJack>,
    range: Range,
    vpo: VoltPerOct,
    quantizer: &Quantizer,
    cv_out: bool,
) {
    if live_note.get() == Some(note) {
        return;
    }
    // Finalize the previous open event so glide steps become short recorded notes.
    if let Some(old) = live_note.get() {
        if armed.get() && rec_open.get() {
            let tick = glob_ticks.get();
            let bars = bars_glob.get();
            let window = LoopBuf::window_ticks(bars).max(1);
            let mut b = buf.get();
            let pos = if b.origin == u64::MAX {
                0
            } else {
                ((tick.saturating_sub(b.origin)) % window as u64) as u16
            };
            let qpos = quantize_16th(pos) % window;
            b.finalize_open(old, qpos, window);
            for e in b.events.iter_mut() {
                if e.used && e.note == old && e.dur == u16::MAX {
                    e.dur = DEFAULT_GATE_TICKS;
                }
            }
            buf.set(b);
            rec_open.set(false);
        }
        midi.send_note_off(MidiNote::from(old)).await;
        live_note.set(None);
    }
    start_note(
        midi,
        buf,
        bars_glob,
        armed,
        live_note,
        rec_open,
        flash_until,
        glob_ticks,
        note,
        vel,
        out_jack,
        range,
        vpo,
        quantizer,
        cv_out,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn note_off(
    midi: &MidiOutput,
    buf: &Global<LoopBuf>,
    bars_glob: &Global<u8>,
    live_note: &Global<Option<u8>>,
    rec_open: &Global<bool>,
    armed: &Global<bool>,
    glob_ticks: &Global<u64>,
    out_jack: Option<&OutJack>,
) {
    if let Some(n) = live_note.get() {
        end_specific(
            midi, buf, bars_glob, live_note, rec_open, armed, glob_ticks, n, out_jack,
        )
        .await;
    } else if let Some(j) = out_jack {
        j.set_value(0);
    }
}

#[allow(clippy::too_many_arguments)]
async fn end_specific(
    midi: &MidiOutput,
    buf: &Global<LoopBuf>,
    bars_glob: &Global<u8>,
    live_note: &Global<Option<u8>>,
    rec_open: &Global<bool>,
    armed: &Global<bool>,
    glob_ticks: &Global<u64>,
    note: u8,
    out_jack: Option<&OutJack>,
) {
    midi.send_note_off(MidiNote::from(note)).await;
    if live_note.get() == Some(note) {
        live_note.set(None);
        if let Some(j) = out_jack {
            j.set_value(0);
        }
    }
    if armed.get() && rec_open.get() {
        let tick = glob_ticks.get();
        let bars = bars_glob.get();
        let window = LoopBuf::window_ticks(bars).max(1);
        let mut b = buf.get();
        let pos = if b.origin == u64::MAX {
            0
        } else {
            ((tick.saturating_sub(b.origin)) % window as u64) as u16
        };
        let qpos = quantize_16th(pos) % window;
        b.finalize_open(note, qpos, window);
        for e in b.events.iter_mut() {
            if e.used && e.note == note && e.dur == u16::MAX {
                e.dur = DEFAULT_GATE_TICKS;
            }
        }
        buf.set(b);
        rec_open.set(false);
    }
}
