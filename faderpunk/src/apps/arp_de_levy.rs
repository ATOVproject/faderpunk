use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    quantizer::Pitch,
    utils::attenuate_bipolar,
    AppIcon, Brightness, ClockDivision, Color, Config, Key, MidiChannel, MidiNote, MidiOut, Note,
    Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::{
    app::{
        App, AppParams, AppStorage, ClockEvent, Die, Led, ManagedStorage, ParamStore, SceneEvent,
    },
    tasks::global_config::get_global_config,
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 12;

/// Reverse gesture LED feedback length (white↔off fade), same as Golden Gate / Heat Pump.
const REVERSE_FADE_MS: u16 = 500;
/// Octave-span cue: one Button blink in the span color (Shift+long).
const OCTAVE_BLINK_MS: u16 = 250;
/// Hold off periodic button LED writes so LedMode::Flash can finish.
const BUTTON_FLASH_MS: u16 = 850;
/// Button latch level floor (Low-ish → High). Wide span so fader motion reads clearly.
const BTN_LEVEL_FLOOR: u8 = 100;
/// Top latch level: deeper dim→bright for amount.
const TOP_LEVEL_FLOOR: u8 = 40;
/// Bottom pitch height floor while gated.
const PITCH_LEVEL_FLOOR: u8 = 90;

const POOL_CAP: usize = 16;
const MIN_PHRASE: usize = 4;
const MAX_PHRASE: usize = 16;
/// Ticks per 16th at 24 PPQN.
const STEP_DIV: u32 = 6;
/// Local Lévy table (**scale degrees**): sticky stepwise motion at α min.
const LEVY_LOCAL_DEG: [i8; 16] = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3];
/// Wild Lévy table (**scale degrees**): larger leaps at α max (~2 octaves in heptatonic).
const LEVY_WILD_DEG: [i8; 16] = [3, 4, 5, 5, 7, 7, 8, 9, 10, 11, 12, 12, 14, 14, 16, 16];
/// Chance (0..=4095) to snap a mutated slot toward the phrase tonic (Contura-ish).
const TONIC_PULL: u16 = 900;
/// Pull α toward 0 / 4095 so Third-layer extremes read clearly (< 1 = stronger).
const ALPHA_LEAN_CURVE: f32 = 0.5;

const OCT_COLORS: [Color; 4] = [Color::Blue, Color::Cyan, Color::Yellow, Color::Red];

const CV_JACK_OUT: usize = 0;
const CV_JACK_IN: usize = 1;
const DEST_MUTATION: usize = 0;
const DEST_TEXTURE: usize = 1;
const DEST_REROLL: usize = 2;
const DEST_COUNT: usize = 3;
/// CV Out jack source (when Jack = CV Out).
const OUT_PITCH: usize = 0;
const OUT_GATE: usize = 1;
const OUT_VELOCITY: usize = 2;
const OUT_COUNT: usize = 3;
const TRIG_HIGH: u16 = 2458;

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 400) as u32 * 4095) / 100) as u16
}

fn mod_u16(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Arp de Levy",
    "Levy-flight generative arpeggiator - evolve, texture, and flight character",
    Color::Rose,
    AppIcon::SoftRandom,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "Base Note" })
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Rose,
        Color::Cyan,
        Color::Blue,
        Color::Green,
        Color::Orange,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::MidiOut)
.add_param(Param::VoltPerOct)
.add_param(Param::bool {
    name: "Bypass quantizer",
})
.add_param(Param::Enum {
    name: "Jack",
    variants: &["CV Out", "CV In"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "CV Out",
    variants: &["Pitch", "Gate", "Velocity"],
})
.add_param(Param::Enum {
    name: "CV Dest",
    variants: &["Evolve", "Texture", "Reroll"],
})
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 400,
})
// Arp already follows the device scale for its degree math; this adds the
// tonic, so the global Tonic fader transposes it with the rest of the set.
.add_param(Param::bool {
    name: "Follow device tonic",
});

pub struct Params {
    midi_channel: MidiChannel,
    note: MidiNote,
    color: Color,
    midi_out: MidiOut,
    vpo: VoltPerOct,
    bypass: bool,
    cv_jack: usize,
    range: Range,
    cv_out: usize,
    cv_dest: usize,
    cv_att: i32,
    follow_tonic: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            note: MidiNote::from(48),
            color: Color::Rose,
            midi_out: MidiOut([true, false, false]), // USB only — all-ports floods cable
            vpo: VoltPerOct::Standard,
            bypass: false,
            cv_jack: CV_JACK_OUT,
            range: Range::_0_10V,
            cv_out: OUT_PITCH,
            cv_dest: DEST_MUTATION,
            cv_att: 100,
            follow_tonic: true,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < 6 {
            return None;
        }
        // Pinned to 11, not PARAMS: this slot layout is fixed history, and a
        // `>= PARAMS` check would silently misread it the moment PARAMS grows.
        let (cv_jack, range, cv_out, cv_dest, cv_att) = if values.len() >= 11 {
            (
                usize::from_value(values[6]).min(1),
                Range::from_value(values[7]),
                usize::from_value(values[8]).min(OUT_COUNT - 1),
                usize::from_value(values[9]).min(DEST_COUNT - 1),
                i32::from_value(values[10]).clamp(0, 400),
            )
        } else if values.len() >= 10 {
            // Pre-CV-Out layout: Jack, Range, Dest, Att.
            (
                usize::from_value(values[6]).min(1),
                Range::from_value(values[7]),
                OUT_PITCH,
                usize::from_value(values[8]).min(DEST_COUNT - 1),
                i32::from_value(values[9]).clamp(0, 400),
            )
        } else {
            (CV_JACK_OUT, Range::_0_10V, OUT_PITCH, DEST_MUTATION, 100)
        };
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            color: Color::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
            vpo: VoltPerOct::from_value(values[4]),
            bypass: bool::from_value(values[5]),
            cv_jack,
            range,
            cv_out,
            cv_dest,
            cv_att,
            // Older blobs predate the flag; default it on like a fresh instance.
            follow_tonic: values.len() < 12 || bool::from_value(values[11]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        vec.push(self.bypass.into()).unwrap();
        vec.push(self.cv_jack.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.cv_out.into()).unwrap();
        vec.push(self.cv_dest.into()).unwrap();
        vec.push(self.cv_att.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Storage {
    /// Main fader: evolve / mutation rate (raw 12-bit).
    fader_saved: u16,
    /// Shift fader: texture macro (raw 12-bit).
    shift_fader_saved: u16,
    /// Button+fader: Lévy α / flight character (raw 12-bit).
    alpha_saved: u16,
    /// Expression depth (0..=4095): how much per-note velocity/gate jitter.
    /// Re-rolled on short-press / CV reroll.
    expression: u16,
    /// Octave span 1..=4 (cycled by Shift+long).
    octaves: u8,
    /// CV Out source 0..=2 (Pitch / Gate / Velocity); Shift+short when Jack=Out.
    cv_out: u8,
    muted: bool,
    reversed: bool,
    /// Persistent note pool as MIDI note numbers.
    pool: [u8; POOL_CAP],
    /// How many pool slots are live (mirrors texture-derived length; persisted).
    phrase_len: u8,
}

impl Default for Storage {
    fn default() -> Self {
        let mut pool = [48u8; POOL_CAP];
        // Seed a simple C-major-ish ascending seed before first Lévy walk.
        for (i, n) in pool.iter_mut().enumerate() {
            *n = 48 + [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23, 24, 26][i];
        }
        Self {
            fader_saved: 0, // frozen by default
            shift_fader_saved: 2048,
            alpha_saved: 2048, // balanced flight
            expression: 0, // deterministic until first reroll
            octaves: 2,
            cv_out: OUT_PITCH as u8,
            muted: false,
            reversed: false,
            pool,
            phrase_len: 8,
        }
    }
}

impl AppStorage for Storage {}

fn clamp_octaves(o: u8) -> u8 {
    o.clamp(1, 4)
}

fn cycle_octaves(o: u8) -> u8 {
    let o = clamp_octaves(o);
    if o >= 4 {
        1
    } else {
        o + 1
    }
}

fn octave_color(octaves: u8) -> Color {
    OCT_COLORS[(clamp_octaves(octaves) - 1) as usize]
}

fn cv_out_color(mode: usize) -> Color {
    match mode.min(OUT_COUNT - 1) {
        OUT_GATE => Color::Yellow,
        OUT_VELOCITY => Color::Orange,
        _ => Color::Red, // Pitch
    }
}

/// Map a 12-bit level into [floor, 255] with sqrt ease-in so early fader travel
/// already brightens (Evolve leaving 0 feels alive; full throw still hits High).
fn level_bright(value: u16, floor: u8) -> Brightness {
    let t = value as f32 / 4095.0;
    let curved = libm::sqrtf(t.clamp(0.0, 1.0));
    let span = (255 - floor) as f32;
    Brightness::Custom((floor as f32 + curved * span) as u8)
}

/// Pitch height within the current octave span → Bottom brightness.
fn pitch_bright(raw: u8, lo: u8, hi: u8) -> Brightness {
    let span = (hi.saturating_sub(lo)).max(1) as u16;
    let pos = (raw.saturating_sub(lo) as u16).min(span);
    Brightness::Custom(
        (PITCH_LEVEL_FLOOR as u16 + pos * (255 - PITCH_LEVEL_FLOOR as u16) / span) as u8,
    )
}

/// Latch layer → hue for Button/Top.
fn latch_hue(layer: LatchLayer, led_color: Color) -> Color {
    match layer {
        LatchLayer::Main => led_color,
        LatchLayer::Alt => Color::Orange,
        LatchLayer::Third => Color::Violet,
    }
}

fn cycle_cv_out(mode: u8) -> u8 {
    ((mode as usize + 1) % OUT_COUNT) as u8
}

/// Texture → (density 0..=4095 hit threshold, phrase_len, swing_ticks).
/// Bottom: sparse / long phrase / no swing. Top: dense / short / swung.
fn texture_from_value(value: u16) -> (u16, usize, u32) {
    let t = value as u32;
    // Hit if die.roll() < density: bottom ≈820 (20%), top ≈4095 (100%).
    let density = (820 + t * (4095 - 820) / 4095) as u16;
    let phrase = MAX_PHRASE - (t as usize * (MAX_PHRASE - MIN_PHRASE) / 4095);
    let phrase = phrase.clamp(MIN_PHRASE, MAX_PHRASE);
    // Swing delay on odd steps: 0 .. ~40% of a step.
    let swing = t * (STEP_DIV * 2 / 5) / 4095;
    (density, phrase, swing)
}

/// Expression depth (0..=4095) → per-note velocity. Base ~78%; jitter scales with depth.
fn express_velocity(die: &Die, expression: u16) -> u16 {
    const BASE: u16 = 3200;
    const MIN_V: u16 = 256;
    if expression == 0 {
        return BASE;
    }
    let delta = ((die.roll() as i32 - 2048) * expression as i32) / 2048;
    (BASE as i32 + delta).clamp(MIN_V as i32, 4095) as u16
}

/// Expression depth → gate length in clock ticks (1..=STEP_DIV-1). Base = half step.
fn express_gate_ticks(die: &Die, expression: u16) -> u32 {
    const BASE: u32 = STEP_DIV / 2;
    if expression == 0 {
        return BASE.max(1);
    }
    let max_j = ((expression as u32) * (STEP_DIV / 2 - 1) / 4095) as i32;
    let delta = ((die.roll() as i32 - 2048) * max_j) / 2048;
    (BASE as i32 + delta).clamp(1, (STEP_DIV - 1) as i32) as u32
}

fn base_midi(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn clamp_note(n: i16) -> u8 {
    n.clamp(0, 127) as u8
}

/// α (0..=4095) blends Local→Wild degree leaps.
fn levy_degree_delta(die: &Die, alpha: u16) -> i8 {
    let t = alpha as f32 / 4095.0;
    let centered = (t - 0.5) * 2.0;
    let lean = libm::copysignf(libm::powf(centered.abs(), ALPHA_LEAN_CURVE), centered);
    let eff = ((0.5 + lean * 0.5) * 4095.0) as u16;
    let mag = if die.roll() < eff {
        LEVY_WILD_DEG[(die.roll() as usize) % LEVY_WILD_DEG.len()]
    } else {
        LEVY_LOCAL_DEG[(die.roll() as usize) % LEVY_LOCAL_DEG.len()]
    };
    if die.roll() & 1 == 0 {
        mag
    } else {
        -mag
    }
}

fn scale_pcs(key: Key) -> Vec<u8, 12> {
    let mask = if key == Key::Off {
        Key::Chromatic.as_u16_key()
    } else {
        key.as_u16_key()
    };
    let mut out = Vec::new();
    for i in 0..12u8 {
        if (mask >> (11 - i)) & 1 != 0 {
            let _ = out.push(i);
        }
    }
    if out.is_empty() {
        let _ = out.push(0);
    }
    out
}

/// Nearest scale degree index for `note` relative to `tonic` pitch-class.
fn note_to_degree(note: u8, tonic: u8, pcs: &[u8]) -> i16 {
    let n = pcs.len().max(1) as i16;
    let pc = ((note as i16 - tonic as i16).rem_euclid(12)) as u8;
    let mut best_i = 0i16;
    let mut best_d = 12u8;
    for (i, &p) in pcs.iter().enumerate() {
        let d = (p as i16 - pc as i16).unsigned_abs() as u8;
        let d = d.min(12 - d);
        if d < best_d {
            best_d = d;
            best_i = i as i16;
        }
    }
    let oct = (note as i16 - tonic as i16).div_euclid(12);
    oct * n + best_i
}

fn degree_to_note(degree: i16, tonic: u8, pcs: &[u8]) -> u8 {
    let n = pcs.len().max(1) as i16;
    let d = degree.rem_euclid(n * 16); // keep in a wide window before clamp
    let oct = d.div_euclid(n);
    let i = d.rem_euclid(n) as usize;
    let semi = tonic as i16 + oct * 12 + pcs[i] as i16;
    semi.clamp(0, 127) as u8
}

fn mutate_pool(
    pool: &mut [u8; POOL_CAP],
    phrase_len: usize,
    lo: u8,
    hi: u8,
    alpha: u16,
    die: &Die,
    tonic_midi: u8,
) {
    if phrase_len == 0 {
        return;
    }
    let key = get_global_config().quantizer.key;
    let pcs = scale_pcs(key);
    let tonic = tonic_midi % 12;
    let i = (die.roll() as usize) % phrase_len;
    let deg = note_to_degree(pool[i], tonic, &pcs);
    let delta = i16::from(levy_degree_delta(die, alpha));
    let mut next_deg = deg + delta;
    // Mild Contura-style tonic pull: nudge toward base after leaps.
    if delta.abs() >= 3 && die.roll() < TONIC_PULL {
        let pull_deg = note_to_degree(tonic_midi.clamp(lo, hi), tonic, &pcs);
        if next_deg > pull_deg {
            next_deg -= 1;
        } else if next_deg < pull_deg {
            next_deg += 1;
        }
    }
    pool[i] = degree_to_note(next_deg, tonic, &pcs).clamp(lo, hi);
}

fn reroll_pool(
    pool: &mut [u8; POOL_CAP],
    phrase_len: usize,
    lo: u8,
    hi: u8,
    die: &Die,
    tonic_midi: u8,
) {
    let key = get_global_config().quantizer.key;
    let pcs = scale_pcs(key);
    let tonic = tonic_midi % 12;
    let base_deg = note_to_degree(tonic_midi.clamp(lo, hi), tonic, &pcs);
    let span_deg = ((hi - lo) as i16 / 2).max(4);
    for (i, slot) in pool.iter_mut().enumerate() {
        if i < phrase_len {
            let wobble = (die.roll() as i16 * span_deg / 4095) - span_deg / 2;
            *slot = degree_to_note(base_deg + wobble, tonic, &pcs).clamp(lo, hi);
        } else {
            *slot = lo;
        }
    }
}

/// At phrase cadence (last slot), bias playback toward tonic for resolution.
fn cadence_note(raw: u8, step: usize, phrase_len: usize, tonic_midi: u8, die: &Die) -> u8 {
    if phrase_len == 0 {
        return raw;
    }
    let last = phrase_len - 1;
    if step % phrase_len == last && die.roll() < 2400 {
        tonic_midi
    } else {
        raw
    }
}

/// Sequential walk through the pool; reverse flips direction.
fn pool_index(step: usize, len: usize, reversed: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let i = step % len;
    if reversed {
        len - 1 - i
    } else {
        i
    }
}

fn note_to_pitch(note: u8) -> Pitch {
    // MIDI note 0 = C-1 → octave -1; MIDI 60 = C4 → octave 4.
    let octave = (note as i16 / 12) - 1;
    let pc = note % 12;
    Pitch {
        octave: octave as i8,
        note: Note::from(pc),
        raw: None,
    }
}

#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id, Params::default());
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
        midi_out,
        midi_chan,
        base_note,
        led_color,
        vpo,
        bypass,
        cv_jack,
        range,
        param_cv_out,
        cv_dest,
        cv_att,
        follow_tonic,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.note,
            p.color,
            p.vpo,
            p.bypass,
            p.cv_jack.min(1),
            p.range,
            p.cv_out.min(OUT_COUNT - 1),
            p.cv_dest.min(DEST_COUNT - 1),
            att_from_pct(p.cv_att),
            p.follow_tonic,
        )
    });

    let glob_ticks = app.make_global(0u64);
    let glob_clock_reset = app.make_global(false);
    let glob_clock_stop = app.make_global(false);
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let die = app.use_die();
    let quantizer = app.use_quantizer(range, vpo, bypass);
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let out_jack = if cv_jack == CV_JACK_OUT {
        Some(app.make_out_jack(0, range).await)
    } else {
        None
    };
    let in_jack = if cv_jack == CV_JACK_IN {
        Some(app.make_in_jack(0, Range::_Neg5_5V).await)
    } else {
        None
    };
    let glob_cv_val = app.make_global(2047u16);

    // Configurator CV Out wins on spawn; keep storage / scenes in sync.
    storage.modify_and_save(|s| s.cv_out = param_cv_out as u8);
    let glob_cv_out = app.make_global(param_cv_out);

    let (
        fader_saved,
        shift_fader_saved,
        alpha_saved,
        expression_saved,
        octaves_saved,
        muted,
        reversed,
        pool_saved,
        phrase_saved,
    ) = storage.query(|s| {
        (
            s.fader_saved,
            s.shift_fader_saved,
            s.alpha_saved,
            s.expression,
            s.octaves,
            s.muted,
            s.reversed,
            s.pool,
            s.phrase_len,
        )
    });

    let glob_muted = app.make_global(muted);
    let glob_reversed = app.make_global(reversed);
    let glob_mutation = app.make_global(fader_saved);
    let glob_texture = app.make_global(shift_fader_saved);
    let glob_alpha = app.make_global(alpha_saved);
    let glob_expression = app.make_global(expression_saved);
    let glob_octaves = app.make_global(clamp_octaves(octaves_saved));
    let glob_phrase =
        app.make_global(phrase_saved.clamp(MIN_PHRASE as u8, MAX_PHRASE as u8) as usize);
    let glob_reset = app.make_global(false);
    let glob_reroll = app.make_global(false);
    let long_press_fired = app.make_global(false);
    let glob_fader_moved = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_reverse_fade = app.make_global(0u16);
    let glob_reverse_fade_up = app.make_global(false);
    let glob_octave_blink = app.make_global(0u16);
    let glob_btn_flash = app.make_global(0u16);
    let glob_reload_pool = app.make_global(false);
    // Root in effect right now — the param's own, or the device tonic when
    // following. The clock future owns it; the voice future reads it.
    let glob_base = app.make_global(base_midi(base_note));
    // Clock watch → voice engine (never await MIDI/quantizer inside the clock subscriber).
    // Same isolation pattern as Chord Vamp — keeps USB MIDI from stalling the clock path.
    let pending_fire = app.make_global(false);
    let pending_raw = app.make_global(0u8);
    let pending_vel = app.make_global(0u16);
    let pending_note_off = app.make_global(false);
    let pending_silence = app.make_global(false);

    // Clear any note left sounding by a prior run() that was dropped mid-gate
    // (e.g. on a param-change respawn) — same MIDI hygiene as Golden Gate.
    midi.send_note_off(base_note).await;
    for n in pool_saved {
        midi.send_note_off(MidiNote::from(n)).await;
    }
    if let Some(ref jack) = out_jack {
        jack.set_value(0);
    }

    if muted {
        leds.unset(0, Led::Button);
    } else {
        let c = latch_hue(LatchLayer::Main, led_color);
        leds.set(
            0,
            Led::Button,
            c,
            level_bright(fader_saved, BTN_LEVEL_FLOOR),
        );
    }

    let (density0, phrase0, _) = texture_from_value(shift_fader_saved);
    let _ = density0;
    glob_phrase.set(phrase0);

    let schedule_hit = |clkn: u32,
                        raw: u8,
                        swing: u32,
                        step: usize,
                        pending_on_at: &mut Option<u32>,
                        pending_note: &mut Option<u8>,
                        gate_off_at: &mut Option<u32>| {
        if glob_muted.get() {
            return;
        }
        if swing > 0 && (step % 2 == 1) {
            *pending_on_at = Some(clkn + swing);
            *pending_note = Some(raw);
        } else {
            let expression = glob_expression.get();
            pending_raw.set(raw);
            pending_vel.set(express_velocity(&die, expression));
            pending_fire.set(true);
            *gate_off_at = Some(clkn + express_gate_ticks(&die, expression));
        }
    };

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
                _ => {}
            }
        }
    };

    let clock_processor = async {
        let mut pool = storage.query(|s| s.pool);
        // The pool holds absolute MIDI notes, so a device transpose has to move
        // it. Shifting keeps the melodic shape the pool has evolved into;
        // rerolling would throw it away. Resolved at phrase starts only —
        // reading the device tonic copies the whole GlobalConfig.
        let shift_pool = |pool: &mut [u8; POOL_CAP], delta: i16| {
            for n in pool.iter_mut() {
                *n = clamp_note(i16::from(*n) + delta);
            }
        };
        let mut cur_base = base_midi(base_note);
        if follow_tonic {
            let want = self::follow_key::root(true, base_note);
            if want != cur_base {
                shift_pool(&mut pool, i16::from(want) - i16::from(cur_base));
                cur_base = want;
                glob_base.set(cur_base);
            }
        }
        let mut step: usize = 0;
        let mut pending_on_at: Option<u32> = None;
        let mut pending_note: Option<u8> = None;
        let mut gate_off_at: Option<u32> = None;

        let mut last_processed_tick: u64 = 0;

        loop {
            app.delay_millis(1).await;

            if glob_clock_reset.get() || glob_clock_stop.get() {
                glob_clock_reset.set(false);
                glob_clock_stop.set(false);
                pending_on_at = None;
                pending_note = None;
                gate_off_at = None;
                pending_fire.set(false);
                pending_note_off.set(false);
                pending_silence.set(true);
                step = 0;
                glob_reset.set(false);
                leds.unset(0, Led::Top);
                leds.unset(0, Led::Bottom);
            }

            let tick = glob_ticks.get();
            if tick == last_processed_tick {
                continue;
            }
            last_processed_tick = tick;
            let clkn = tick as u32;

                    let octaves = clamp_octaves(glob_octaves.get());
                    let lo = cur_base;
                    let hi = clamp_note(lo as i16 + (octaves as i16) * 12);
                    let texture_val = if cv_jack == CV_JACK_IN && cv_dest == DEST_TEXTURE {
                        mod_u16(glob_texture.get(), glob_cv_val.get())
                    } else {
                        glob_texture.get()
                    };
                    let (density, phrase_len, swing) = texture_from_value(texture_val);
                    glob_phrase.set(phrase_len);

                    if glob_reload_pool.get() {
                        glob_reload_pool.set(false);
                        pool = storage.query(|s| s.pool);
                    }

                    if glob_reroll.get() {
                        glob_reroll.set(false);
                        reroll_pool(&mut pool, phrase_len, lo, hi, &die, lo);
                        // Short/CV reroll also picks a new expression depth.
                        let expression = die.roll();
                        glob_expression.set(expression);
                        storage.modify_and_save(|s| {
                            s.pool = pool;
                            s.phrase_len = phrase_len as u8;
                            s.expression = expression;
                        });
                    }

                    // Gate off — flag only; voice engine owns MIDI.
                    if let Some(off_at) = gate_off_at {
                        if clkn >= off_at {
                            pending_note_off.set(true);
                            gate_off_at = None;
                        }
                    }

                    // Delayed (swung) note-on
                    if let Some(on_at) = pending_on_at {
                        if clkn >= on_at {
                            if let Some(raw) = pending_note.take() {
                                if !glob_muted.get() {
                                    let expression = glob_expression.get();
                                    pending_raw.set(raw);
                                    pending_vel.set(express_velocity(&die, expression));
                                    pending_fire.set(true);
                                    gate_off_at =
                                        Some(on_at + express_gate_ticks(&die, expression));
                                }
                            }
                            pending_on_at = None;
                        }
                    }

                    if clkn.is_multiple_of(STEP_DIV) {
                        if glob_reset.get() {
                            glob_reset.set(false);
                            step = 0;
                        }

                        // At phrase boundary: Lévy-mutate according to evolve rate + α.
                        if step == 0 {
                            let mut mutation = if cv_jack == CV_JACK_IN && cv_dest == DEST_MUTATION {
                                mod_u16(glob_mutation.get(), glob_cv_val.get())
                            } else {
                                glob_mutation.get()
                            };
                            let alpha = glob_alpha.get();
                            let mut changed = false;
                            // Number of mutations scales with fader (0 = freeze).
                            while mutation > 0 {
                                if die.roll() < mutation {
                                    mutate_pool(&mut pool, phrase_len, lo, hi, alpha, &die, lo);
                                    changed = true;
                                }
                                mutation = mutation.saturating_sub(1024);
                            }
                            if changed {
                                storage.modify_and_save(|s| {
                                    s.pool = pool;
                                    s.phrase_len = phrase_len as u8;
                                });
                            }
                        }

                        let reversed = glob_reversed.get();
                        let idx = pool_index(step, phrase_len, reversed);
                        let raw = cadence_note(
                            pool[idx].clamp(lo, hi),
                            step,
                            phrase_len,
                            lo,
                            &die,
                        );

                        // Density: rest if roll >= density threshold.
                        let hit = die.roll() < density && !glob_muted.get();

                        if hit {
                            schedule_hit(
                                clkn,
                                raw,
                                swing,
                                step,
                                &mut pending_on_at,
                                &mut pending_note,
                                &mut gate_off_at,
                            );
                        }

                        step = step.wrapping_add(1);
                        if phrase_len > 0 && step.is_multiple_of(phrase_len) {
                            step = 0;
                            if follow_tonic {
                                let want = self::follow_key::root(true, base_note);
                                if want != cur_base {
                                    shift_pool(
                                        &mut pool,
                                        i16::from(want) - i16::from(cur_base),
                                    );
                                    cur_base = want;
                                    glob_base.set(cur_base);
                                }
                            }
                        }
                    }
        }
    };

    let voice = async {
        let mut note_on: Option<MidiNote> = None;
        loop {
            app.delay_millis(1).await;

            if pending_silence.get() {
                pending_silence.set(false);
                pending_fire.set(false);
                pending_note_off.set(false);
                if let Some(n) = note_on.take() {
                    midi.send_note_off(n).await;
                }
                if let Some(ref jack) = out_jack {
                    jack.set_value(0);
                }
                leds.set(0, Led::Bottom, led_color, Brightness::Off);
                continue;
            }

            if pending_note_off.get() {
                pending_note_off.set(false);
                if let Some(n) = note_on.take() {
                    midi.send_note_off(n).await;
                }
                // Gate / Velocity drop with the note; Pitch holds until next hit / silence.
                if glob_cv_out.get() != OUT_PITCH {
                    if let Some(ref jack) = out_jack {
                        jack.set_value(0);
                    }
                }
                leds.set(0, Led::Bottom, led_color, Brightness::Off);
            }

            if pending_fire.get() {
                pending_fire.set(false);
                if !glob_muted.get() {
                    let octaves = clamp_octaves(glob_octaves.get());
                    let lo = glob_base.get();
                    let hi = clamp_note(lo as i16 + (octaves as i16) * 12);
                    let raw = pending_raw.get();
                    fire_note(
                        &midi,
                        out_jack.as_ref(),
                        &quantizer,
                        &leds,
                        led_color,
                        vpo,
                        range,
                        glob_cv_out.get(),
                        raw,
                        pending_vel.get(),
                        pitch_bright(raw, lo, hi),
                        &mut note_on,
                    )
                    .await;
                }
            }
        }
    };

    let fut_buttons = async {
        loop {
            buttons.wait_for_any_down().await;
            if buttons.is_shift_pressed() {
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    if cv_jack == CV_JACK_OUT {
                        // Shift + short (CV Out): cycle Pitch / Gate / Velocity + flash.
                        let next = storage.modify_and_save(|s| {
                            s.cv_out = cycle_cv_out(s.cv_out);
                            s.cv_out
                        });
                        let next = next as usize;
                        glob_cv_out.set(next);
                        glob_reverse_fade.set(0);
                        glob_octave_blink.set(0);
                        leds.set_mode(
                            0,
                            Led::Button,
                            LedMode::Flash(cv_out_color(next), Some(3)),
                        );
                        glob_btn_flash.set(BUTTON_FLASH_MS);
                    } else {
                        // Shift + short (CV In): reverse playback direction.
                        let reversed = glob_reversed.toggle();
                        storage.modify_and_save(|s| s.reversed = reversed);
                        glob_octave_blink.set(0);
                        glob_reverse_fade_up.set(!reversed);
                        glob_reverse_fade.set(REVERSE_FADE_MS);
                    }
                }
            } else {
                long_press_fired.set(false);
                glob_fader_moved.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    // Short press: reroll pool + expression depth (vel/gate jitter).
                    glob_reroll.set(true);
                    glob_reset.set(true);
                } else if !glob_fader_moved.get() {
                    // Long press without fader move: mute.
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| s.muted = muted);
                    if muted {
                        pending_silence.set(true);
                        leds.unset(0, Led::Button);
                        leds.unset(0, Led::Bottom);
                    }
                    // Unmute: Button restored by shift loop via latch_level.
                }
            }
        }
    };

    let long_press = async {
        loop {
            buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);

            if buttons.is_shift_pressed() {
                // Shift + long: cycle octave span 1→2→3→4.
                let octaves = cycle_octaves(glob_octaves.get());
                glob_octaves.set(octaves);
                storage.modify_and_save(|s| s.octaves = octaves);
                // Top cue + one Button blink in the span color.
                leds.set(0, Led::Top, octave_color(octaves), Brightness::High);
                glob_reverse_fade.set(0);
                glob_btn_flash.set(0);
                glob_octave_blink.set(OCTAVE_BLINK_MS);
            }
        }
    };

    let fut_faders = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change_at(0).await;
            let latch_layer = glob_latch_layer.get();

            if latch_layer == LatchLayer::Third {
                glob_fader_moved.set(true);
            }

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.fader_saved),
                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved),
                LatchLayer::Third => storage.query(|s| s.alpha_saved),
            };

            if let Some(new_value) = latch.update(faders.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_mutation.set(new_value);
                        storage.modify_and_save(|s| s.fader_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        glob_texture.set(new_value);
                        let (_, phrase, _) = texture_from_value(new_value);
                        glob_phrase.set(phrase);
                        storage.modify_and_save(|s| {
                            s.shift_fader_saved = new_value;
                            s.phrase_len = phrase as u8;
                        });
                    }
                    LatchLayer::Third => {
                        glob_fader_moved.set(true);
                        glob_alpha.set(new_value);
                        storage.modify_and_save(|s| s.alpha_saved = new_value);
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (
                        fader_saved,
                        shift_fader_saved,
                        alpha_saved,
                        expression,
                        octaves,
                        cv_out,
                        muted,
                        reversed,
                    ) = storage.query(|s| {
                        (
                            s.fader_saved,
                            s.shift_fader_saved,
                            s.alpha_saved,
                            s.expression,
                            s.octaves,
                            s.cv_out,
                            s.muted,
                            s.reversed,
                        )
                    });
                    glob_mutation.set(fader_saved);
                    glob_texture.set(shift_fader_saved);
                    glob_alpha.set(alpha_saved);
                    glob_expression.set(expression);
                    glob_octaves.set(clamp_octaves(octaves));
                    let cv_out = (cv_out as usize).min(OUT_COUNT - 1);
                    glob_cv_out.set(cv_out);
                    glob_muted.set(muted);
                    glob_reversed.set(reversed);
                    let (_, phrase, _) = texture_from_value(shift_fader_saved);
                    glob_phrase.set(phrase);
                    glob_reroll.set(false);
                    glob_reload_pool.set(true);
                    glob_reset.set(true);

                    if muted {
                        midi.send_note_off(base_note).await;
                        leds.unset(0, Led::Button);
                    }
                    // Unmute / level: Button restored by shift loop via latch_level.
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let shift = async {
        let mut prev_gate_high = false;
        loop {
            app.delay_millis(1).await;
            if let Some(ref input) = in_jack {
                let in_val = attenuate_bipolar(input.get_value(), cv_att);
                glob_cv_val.set(in_val);
                if cv_dest == DEST_REROLL {
                    let high = in_val >= TRIG_HIGH;
                    if high && !prev_gate_high {
                        glob_reroll.set(true);
                        glob_reset.set(true);
                    }
                    prev_gate_high = high;
                } else {
                    prev_gate_high = false;
                }
            }
            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);

            // Live fader → brightness so scrubbing shows a real gradient immediately
            // (params still go through the latch in fut_faders).
            let layer_color = latch_hue(latch_active_layer, led_color);
            let fader_now = faders.get_value();
            let btn_bright = level_bright(fader_now, BTN_LEVEL_FLOOR);
            let top_bright = level_bright(fader_now, TOP_LEVEL_FLOOR);

            // Top: deeper dim→bright. Button: Low→High with sqrt ease.
            leds.set(0, Led::Top, layer_color, top_bright);

            // Reverse / octave / CV-flash cues own the Button briefly; else latch level.
            let fade_left = glob_reverse_fade.get();
            let blink_left = glob_octave_blink.get();
            let flash_left = glob_btn_flash.get();
            if flash_left > 0 {
                glob_btn_flash.set(flash_left.saturating_sub(1));
            } else if fade_left > 0 {
                let elapsed = REVERSE_FADE_MS.saturating_sub(fade_left);
                let bright = if glob_reverse_fade_up.get() {
                    ((elapsed as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                } else {
                    (((REVERSE_FADE_MS - elapsed) as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                };
                leds.set(0, Led::Button, Color::White, Brightness::Custom(bright));
                glob_reverse_fade.set(fade_left.saturating_sub(1));
            } else if blink_left > 0 {
                // Octave-span cue: one Button blink in Blue/Cyan/Yellow/Red.
                let bright =
                    ((blink_left as u32 * 255) / OCTAVE_BLINK_MS as u32).min(255) as u8;
                leds.set(
                    0,
                    Led::Button,
                    octave_color(glob_octaves.get()),
                    Brightness::Custom(bright),
                );
                glob_octave_blink.set(blink_left.saturating_sub(1));
            } else if glob_muted.get() {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, layer_color, btn_bright);
            }
        }
    };

    join(
        long_press,
        join5(
            join(join(clock_drain, clock_processor), voice),
            fut_buttons,
            fut_faders,
            scene_handler,
            shift,
        ),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn fire_note(
    midi: &crate::app::MidiOutput,
    cv_jack: Option<&crate::app::OutJack>,
    quantizer: &crate::app::Quantizer,
    leds: &crate::app::Leds<CHANNELS>,
    led_color: Color,
    vpo: VoltPerOct,
    out_range: Range,
    cv_out: usize,
    raw: u8,
    velocity: u16,
    bottom_bright: Brightness,
    note_on: &mut Option<MidiNote>,
) {
    // Quantize via 1V/oct counts derived from the MIDI note, then emit MIDI
    // (and CV according to CV Out mode — same dual-path idea as GenSeq).
    let pitch = note_to_pitch(raw);
    let counts = pitch.as_counts(out_range, vpo);
    let q = quantizer.get_quantized_note(counts).await;
    let out_counts = q.as_counts(out_range, vpo);
    if let Some(jack) = cv_jack {
        match cv_out.min(OUT_COUNT - 1) {
            OUT_GATE => jack.set_value(4095),
            OUT_VELOCITY => jack.set_value(velocity),
            _ => jack.set_value(out_counts), // Pitch
        }
    }

    let midi_n = q.as_midi();
    // Prefer quantized pitch; fall back to raw if bypass leaves us at 0.
    let n = if u7::from(midi_n).as_int() == 0 {
        MidiNote::from(raw)
    } else {
        midi_n
    };

    if let Some(prev) = note_on.take() {
        midi.send_note_off(prev).await;
    }
    midi.send_note_on(n, velocity).await;
    *note_on = Some(n);
    // Bottom: pitch height within octave span while the gate is open.
    leds.set(0, Led::Bottom, led_color, bottom_bright);
}

mod follow_key {
    use libfp::MidiNote;
    use midly::num::u7;

    use crate::tasks::global_config::get_global_config;

    pub fn root(follow: bool, local_root: MidiNote) -> u8 {
        if follow {
            retune(local_root, get_global_config().quantizer.tonic as u8)
        } else {
            midi_u8(local_root)
        }
    }

    pub fn retune(root: MidiNote, pc: u8) -> u8 {
        let n = midi_u8(root);
        let target = (n - n % 12) + pc % 12;
        if target > 127 {
            target - 12
        } else {
            target
        }
    }

    fn midi_u8(note: MidiNote) -> u8 {
        u7::from(note).as_int()
    }
}

