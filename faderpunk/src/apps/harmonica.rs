//! Harmonica — monophonic **input**, chord **output**.
//!
//! One root (last-note MIDI or a single pitch CV) selects the chord root.
//! The fader chooses the chord type; up to four MIDI voices sound (and
//! MIDI→CV follows the highest). Not a polyphonic keyboard input.
//!
//! CV→MIDI uses one jack (pitch only). Pair with Note Fader on **0–10V**
//! (same Range). Config **CV Gate**: Sustain, fixed ms pulse, or clock
//! divisions. Near-0V CV = rest (silence). Unpatched float stays silent
//! until ADC motion then a stable pitch.

use embassy_futures::{
    join::{join, join4},
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use portable_atomic::{AtomicU64, Ordering};
use heapless::Vec;
use midly::{num::u7, MidiMessage};
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    quantizer::Pitch,
    utils::{value_to_index},
    AppIcon, Brightness, Color, Config, MidiChannel, MidiIn, MidiNote, MidiOut, Note, Param, Range,
    Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::ClockEvent;
use crate::tasks::clock::CLOCK_PUBSUB;

use crate::app::{
    App, AppParams, AppStorage, Global, Led, ManagedStorage, MidiOutput, OutJack, ParamStore,
    Quantizer, SceneEvent,
};


mod led_spectrum {
    use libfp::{utils::split_unsigned_value, Brightness, Color};
    use smart_leds::RGB8;

    use crate::app::{Led, Leds};

    pub fn hsv_to_rgb(hue: u16, sat: u8, val: u8) -> (u8, u8, u8) {
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

    pub fn spectrum_color(fader: u16) -> Color {
        let hue = (fader.min(4095) as u32 * 270 / 4095) as u16;
        let (r, g, b) = hsv_to_rgb(hue, 255, 255);
        Color::Custom(r, g, b)
    }

    pub fn paint_fader_meters<const N: usize>(
        leds: &Leds<N>,
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

    #[allow(dead_code)]
    pub fn blend_colors(a: Color, b: Color, fader: u16) -> Color {
        let frac = (fader.min(4095) as u32 * 255) / 4095;
        let aa: RGB8 = a.into();
        let bb: RGB8 = b.into();
        let lerp = |x: u8, y: u8| ((x as u32 * (255 - frac) + y as u32 * frac) / 255) as u8;
        Color::Custom(lerp(aa.r, bb.r), lerp(aa.g, bb.g), lerp(aa.b, bb.b))
    }
}

use self::led_spectrum::{paint_fader_meters, spectrum_color};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 11;

static HARMONICA_TICK: AtomicU64 = AtomicU64::new(0);

fn harmonica_ticks() -> u64 {
    HARMONICA_TICK.load(Ordering::Relaxed)
}

async fn clock_tick_drain() {
    let mut sub = CLOCK_PUBSUB.subscriber().unwrap();
    loop {
        match sub.next_message_pure().await {
            ClockEvent::Tick(t) => HARMONICA_TICK.store(t, Ordering::Relaxed),
            ClockEvent::Start | ClockEvent::Reset => HARMONICA_TICK.store(0, Ordering::Relaxed),
            ClockEvent::Stop => {}
        }
    }
}


const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Mid→Low button duck on each harmony trigger — same length as Heat Pump / Grooves.
const BUTTON_DUCK_MS: u16 = 25;
const NUM_CHORD_TYPES: usize = 10;
const MAX_VOICES: usize = 4;
/// Held MIDI keys for last-note root priority. Input is mono (one root); output
/// is still a multi-voice chord. Gate stays high while any key is held.
const MAX_HELD: usize = 8;
const SPREAD_STEPS: usize = 4;

const IO_MIDI_MIDI: usize = 0;
const IO_MIDI_CV: usize = 1;
const IO_CV_MIDI: usize = 2;

/// CV→MIDI: quantized note must hold this many 1 ms frames to count as stable.
const CV_STABLE_MS: u16 = 12;
/// Sliding window for peak-to-peak "slew / unplug" detection.
const CV_HIST: usize = 32;
/// Peak-to-peak → mark motion (patch / Note Fader step) while Idle/Armed.
const CV_MOTION_PP: u16 = 96;
/// Peak-to-peak to leave Playing — higher so held pitch CV doesn't chatter off.
const CV_RETRIG_PP: u16 = 320;
/// Ms of high retrig noise before we drop the chord (unplug).
const CV_UNPLUG_MS: u16 = 200;
/// After ADC chaos (plug / unplug / slew), suppress note-starts this long once quiet.
/// Stops contact-bounce from locking random mid-transition pitches.
const CV_SETTLE_BLANK_MS: u16 = 100;
/// Below this ADC count ≈ cable at 0V / Note Fader released (0–10V). Not mid-rail float.
const CV_REST_COUNTS: u16 = 80;
/// Floor so Third-fader-at-bottom can't emit MIDI note-on vel 0 (= note-off).
const VEL_FLOOR: u16 = 512;

/// CV Gate param indices (Enum).
const GATE_SUSTAIN: usize = 0;
const GATE_MODE_MAX: usize = 9;
/// Per-voice strum delay in ms; 0 = all voices at once.
const STRUM_MAX_MS: u16 = 200;

/// CV→MIDI voice state.
/// Quiet boot/float → Armed (silent). After ADC motion (patch / Note Fader
/// trigger / slew) the next stable pitch plays — so a single stepped CV
/// update is enough; we do not require a second pitch change.
#[derive(Clone, Copy, PartialEq)]
enum CvVoice {
    Idle,
    /// Stable pitch with no prior motion — kills unpatched mid-rail drones.
    Armed {
        note: u8,
    },
    /// Sounding; brief noise keeps us live so stepped CV still retriggers.
    Playing {
        note: u8,
    },
    /// Was playing, ADC moving — next stable note resumes Playing.
    Slewing,
}

/// Semitone offsets from root (index 0 is always the root).
/// Fader maps across these; new types are appended so wire indices for the
/// original seven stay in the same relative order (bucket widths change).
const CHORD_TEMPLATES: &[&[i8]] = &[
    &[0],            // Unison
    &[0, 7],         // Power
    &[0, 3, 7],      // Minor
    &[0, 4, 7],      // Major
    &[0, 5, 7],      // Sus4
    &[0, 3, 7, 10],  // Min7
    &[0, 4, 7, 10],  // Dom7
    &[0, 4, 7, 11],  // Maj7
    &[0, 3, 7, 10, 14], // Min9 (cap to MAX_VOICES below)
    &[0, 4, 7, 14],  // Add9
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "Harmonica",
    "One MIDI/CV root -> chord out",
    Color::Orange,
    AppIcon::Note,
)
.add_param(Param::Enum {
    name: "I/O",
    variants: &["MIDI->MIDI", "MIDI->CV", "CV->MIDI"],
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
.add_param(Param::MidiChannel {
    name: "MIDI Out CH",
})
.add_param(Param::VoltPerOct)
.add_param(Param::bool {
    name: "Bypass quantizer",
})
.add_param(Param::Enum {
    name: "CV Gate",
    variants: &[
        "Sustain", "50 ms", "100 ms", "200 ms", "500 ms", "1/16", "1/8", "1/4", "1/2", "1",
    ],
})
.add_param(Param::i32 {
    name: "Strum ms",
    min: 0,
    max: STRUM_MAX_MS as i32,
});

pub struct Params {
    io_mode: usize,
    range: Range,
    color: Color,
    midi_in: MidiIn,
    midi_in_ch: MidiChannel,
    midi_out: MidiOut,
    midi_out_ch: MidiChannel,
    vpo: VoltPerOct,
    bypass: bool,
    /// CV→MIDI gate: Sustain | ms pulse | clock division (see GATE_*).
    gate_mode: usize,
    /// Delay between chord voices on each new chord (0 = all at once).
    strum_ms: u16,
}

/// Strum shipped briefly as an Enum of fixed steps before becoming a free ms
/// slider — map those stored indices onto their old millisecond values.
fn migrate_strum_ms(v: &Value) -> u16 {
    match *v {
        Value::i32(ms) => ms.clamp(0, STRUM_MAX_MS as i32) as u16,
        Value::Enum(i) => match i {
            1 => 10,
            2 => 20,
            3 => 30,
            4 => 50,
            5 => 80,
            6 => 120,
            _ => 0,
        },
        _ => 0,
    }
}

fn migrate_gate_mode(v: &Value) -> usize {
    match *v {
        Value::Enum(i) => i.min(GATE_MODE_MAX),
        // Old i32 "CV Gate ms" scenes.
        Value::i32(ms) => match ms.clamp(0, 2000) {
            0 => GATE_SUSTAIN,
            1..=74 => 1,
            75..=149 => 2,
            150..=349 => 3,
            _ => 4,
        },
        _ => 3,
    }
}

fn gate_time_ms(mode: usize) -> Option<u16> {
    match mode {
        1 => Some(50),
        2 => Some(100),
        3 => Some(200),
        4 => Some(500),
        _ => None,
    }
}

/// Voicing presets stepped with Alt+long press: (spread step, harmony octave
/// index, strum ms). Spread steps 0–2 = root / 1st / 2nd inversion; 3 = open.
/// Ordered from tight/dry to wide/slow so walking the list is a gradual opening.
const PRESETS: [(usize, u8, u16); NUM_PRESETS] = [
    (0, 1, 0),   // Close — block chord, no strum
    (0, 0, 0),   // Close Low
    (0, 2, 0),   // Close High
    (1, 1, 0),   // Open
    (2, 1, 0),   // Wide
    (0, 1, 10),  // Pluck
    (1, 1, 30),  // Guitar
    (1, 0, 30),  // Guitar Low
    (2, 1, 50),  // Harp
    (2, 1, 80),  // Roll
    (3, 2, 30),  // Bells
    (3, 1, 120), // Cascade
];
const NUM_PRESETS: usize = 12;
/// Single solid flash after a preset step (~1 ms frames).
const PRESET_FLASH_MS: u16 = 220;
/// Hold length for Alt+long preset step (matches hardware LONG_PRESS).
const ALT_LONG_MS: u64 = 500;

/// Center of a spread bucket, so the stored value survives `value_to_index`.
fn spread_value(step: usize) -> u16 {
    ((step * 4096 + 2048) / SPREAD_STEPS).min(4095) as u16
}

/// PPQN ticks at 24 PPQN (1/4 = 24).
fn gate_clock_ticks(mode: usize) -> Option<u64> {
    match mode {
        5 => Some(6),  // 1/16
        6 => Some(12), // 1/8
        7 => Some(24), // 1/4
        8 => Some(48), // 1/2
        9 => Some(96), // 1 bar / whole
        _ => None,
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Accept 9-param scenes (pre–Gate) and default 200 ms.
        if values.len() < 9 {
            return None;
        }
        Some(Self {
            io_mode: usize::from_value(values[0]),
            range: Range::from_value(values[1]),
            color: Color::from_value(values[2]),
            midi_in: MidiIn::from_value(values[3]),
            midi_in_ch: MidiChannel::from_value(values[4]),
            midi_out: MidiOut::from_value(values[5]),
            midi_out_ch: MidiChannel::from_value(values[6]),
            vpo: VoltPerOct::from_value(values[7]),
            bypass: bool::from_value(values[8]),
            gate_mode: if values.len() > 9 {
                migrate_gate_mode(&values[9])
            } else {
                3
            },
            strum_ms: if values.len() > 10 {
                migrate_strum_ms(&values[10])
            } else {
                0
            },
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.io_mode.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.midi_in_ch.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_out_ch.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        vec.push(self.bypass.into()).unwrap();
        vec.push(Value::Enum(self.gate_mode.min(GATE_MODE_MAX)))
            .unwrap();
        vec.push(Value::i32(self.strum_ms.min(STRUM_MAX_MS) as i32))
            .unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    chord_saved: u16,
    spread_saved: u16,
    vel_saved: u16,
    /// 0 = −1 oct, 1 = 0, 2 = +1
    octave_idx: u8,
    muted: bool,
    /// Last stepped voicing preset, or `NUM_PRESETS` for "none — use params".
    preset_idx: u8,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            chord_saved: 0,
            spread_saved: 0,
            vel_saved: 4095,
            octave_idx: 1,
            muted: false,
            preset_idx: NUM_PRESETS as u8,
        }
    }
}

impl AppStorage for Storage {}

fn midi_u8(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn midi_to_pitch(midi: u8) -> Pitch {
    Pitch {
        octave: (midi as i32 / 12 - 1) as i8,
        note: Note::from(midi % 12),
        raw: None,
    }
}

fn octave_from_idx(idx: u8) -> i8 {
    match idx {
        0 => -1,
        2 => 1,
        _ => 0,
    }
}

fn unique_push(out: &mut Vec<u8, MAX_VOICES>, note: u8) {
    if out.contains(&note) {
        return;
    }
    let _ = out.push(note);
}

struct VoiceParams {
    root: u8,
    chord_type: usize,
    spread: u16,
    octave_idx: u8,
    muted: bool,
    range: Range,
    vpo: VoltPerOct,
    bypass: bool,
}

/// Classic inversion: lift the bottom voice an octave `inv` times.
fn invert_voicing(notes: &mut Vec<u8, MAX_VOICES>, inv: usize) {
    let n = notes.len();
    if n <= 1 || inv == 0 {
        return;
    }
    let inv = inv.min(n - 1);
    for _ in 0..inv {
        let low = notes[0];
        for i in 0..n - 1 {
            notes[i] = notes[i + 1];
        }
        notes[n - 1] = low.saturating_add(12).min(127);
    }
}

/// Chromatic chord template from a single root, then snap non-root tones to scale.
/// Alt spread: steps 0–2 = root / 1st / 2nd inversion; step 3 = open (top + octave).
async fn build_voices(quantizer: &Quantizer, p: VoiceParams) -> Vec<u8, MAX_VOICES> {
    let mut out: Vec<u8, MAX_VOICES> = Vec::new();
    if p.muted {
        // Full mute — silence root and harmony (button LED off).
        return out;
    }
    unique_push(&mut out, p.root);

    let template = CHORD_TEMPLATES[p.chord_type.min(NUM_CHORD_TYPES - 1)];
    let spread_steps = value_to_index(p.spread, SPREAD_STEPS);
    let harm_oct = octave_from_idx(p.octave_idx) as i16 * 12;

    for &semis in template.iter() {
        if semis == 0 {
            continue;
        }
        if out.len() >= MAX_VOICES {
            break;
        }
        let mut note = (p.root as i16 + semis as i16).clamp(0, 127) as u8;
        if !p.bypass {
            let counts = midi_to_pitch(note).as_counts(p.range, p.vpo);
            note = midi_u8(quantizer.get_quantized_note(counts).await.as_midi());
        }
        note = (note as i16 + harm_oct).clamp(0, 127) as u8;
        unique_push(&mut out, note);
    }

    // Sort ascending so inversion rotates true bass→tenor→alto.
    for i in 1..out.len() {
        let mut j = i;
        while j > 0 && out[j] < out[j - 1] {
            out.swap(j, j - 1);
            j -= 1;
        }
    }

    let open = spread_steps >= SPREAD_STEPS.saturating_sub(1);
    let inv = if open {
        0
    } else {
        spread_steps.min(out.len().saturating_sub(1))
    };
    invert_voicing(&mut out, inv);
    if open {
        if let Some(top) = out.last_mut() {
            *top = (*top as u16 + 12).min(127) as u8;
        }
    }
    out
}

/// Note-ons waiting for their strum slot: (note, frames left, velocity).
type StrumQueue = Vec<(u8, u16, u16), MAX_VOICES>;

#[allow(clippy::too_many_arguments)]
async fn revoice(
    midi: &MidiOutput,
    old: &Vec<u8, MAX_VOICES>,
    new: &Vec<u8, MAX_VOICES>,
    root: Option<u8>,
    root_vel: u16,
    harm_vel: u16,
    strum_ms: u16,
    pending: &mut StrumQueue,
) {
    let root_vel = root_vel.max(VEL_FLOOR);
    let harm_vel = harm_vel.max(VEL_FLOOR);
    for &n in old.iter() {
        if !new.contains(&n) {
            midi.send_note_off(MidiNote::from(n)).await;
        }
    }
    // Voices dropped by a chord change must not arrive late.
    let mut i = 0;
    while i < pending.len() {
        if new.contains(&pending[i].0) {
            i += 1;
        } else {
            let _ = pending.remove(i);
        }
    }
    let mut slot = 0u16;
    for &n in new.iter() {
        if old.contains(&n) || pending.iter().any(|&(p, _, _)| p == n) {
            continue;
        }
        let vel = if Some(n) == root { root_vel } else { harm_vel };
        if strum_ms == 0 {
            midi.send_note_on(MidiNote::from(n), vel).await;
        } else {
            // Voices are built root-first, so the queue strums upward.
            let _ = pending.push((n, slot.saturating_mul(strum_ms), vel));
            slot = slot.saturating_add(1);
        }
    }
}

/// Fire any strummed note-ons that came due this frame (~1 ms).
async fn drain_strum(midi: &MidiOutput, pending: &mut StrumQueue) {
    let mut i = 0;
    while i < pending.len() {
        if pending[i].1 == 0 {
            let (note, _, vel) = pending[i];
            let _ = pending.remove(i);
            midi.send_note_on(MidiNote::from(note), vel).await;
        } else {
            pending[i].1 -= 1;
            i += 1;
        }
    }
}

async fn all_notes_off(
    midi: &MidiOutput,
    sounding: &mut Vec<u8, MAX_VOICES>,
    pending: &mut StrumQueue,
) {
    pending.clear();
    for &n in sounding.iter() {
        midi.send_note_off(MidiNote::from(n)).await;
    }
    sounding.clear();
}

/// Push/move key to end — most recent NoteOn becomes the chord root.
fn held_note_on(held: &mut Vec<u8, MAX_HELD>, key: u8) {
    if let Some(i) = held.iter().position(|&k| k == key) {
        let _ = held.remove(i);
    }
    if held.is_full() {
        let _ = held.remove(0);
    }
    let _ = held.push(key);
}

fn held_note_off(held: &mut Vec<u8, MAX_HELD>, key: u8) -> bool {
    if let Some(i) = held.iter().position(|&k| k == key) {
        let _ = held.remove(i);
        true
    } else {
        false
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_midi_note_off(
    midi: &MidiOutput,
    held: &mut Vec<u8, MAX_HELD>,
    sounding: &mut Vec<u8, MAX_VOICES>,
    pending: &mut StrumQueue,
    root_glob: &Global<Option<u8>>,
    revoice_flag: &Global<bool>,
    out_jack: Option<&OutJack>,
    key_n: u8,
) {
    if !held_note_off(held, key_n) {
        // Echo NoteOff for a harmony we never tracked — ignore.
        // Desync safety: NoteOff for current root with empty stack still kills.
        if root_glob.get() == Some(key_n) && held.is_empty() {
            all_notes_off(midi, sounding, pending).await;
            root_glob.set(None);
            if let Some(jack) = out_jack {
                jack.set_value(0);
            }
        }
        return;
    }
    if let Some(&last) = held.last() {
        // Still held keys → gate stays high; fall back to prior note.
        if root_glob.get() != Some(last) {
            root_glob.set(Some(last));
            revoice_flag.set(true);
        }
    } else {
        all_notes_off(midi, sounding, pending).await;
        root_glob.set(None);
        if let Some(jack) = out_jack {
            jack.set_value(0);
        }
    }
}

#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            io_mode: IO_MIDI_MIDI,
            range: Range::_0_10V,
            color: Color::Orange,
            midi_in: MidiIn::default(),
            midi_in_ch: MidiChannel::default(),
            midi_out: MidiOut([true, false, false]), // USB only — all-ports floods cable
            midi_out_ch: MidiChannel::from(2),
            vpo: VoltPerOct::Standard,
            bypass: false,
            gate_mode: 3, // 200 ms
            strum_ms: 0,
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
        range,
        led_color,
        midi_in_cfg,
        midi_in_ch,
        midi_out_cfg,
        midi_out_ch,
        vpo,
        bypass,
        gate_mode,
        strum_ms,
    ) = params.query(|p| {
        (
            p.io_mode,
            p.range,
            p.color,
            p.midi_in,
            p.midi_in_ch,
            p.midi_out,
            p.midi_out_ch,
            p.vpo,
            p.bypass,
            p.gate_mode.min(GATE_MODE_MAX),
            p.strum_ms.min(STRUM_MAX_MS),
        )
    });

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut midi_in = app.use_midi_input(midi_in_cfg, midi_in_ch);
    let midi = app.use_midi_output(midi_out_cfg, midi_out_ch, false);
    let quantizer = app.use_quantizer(range, vpo, bypass);
    // Absolute tick for clock-gated CV — drained in `clock_tick_drain`.
    let gate_ms = gate_time_ms(gate_mode);
    let gate_ticks = gate_clock_ticks(gate_mode);

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

    let (chord_saved, spread_saved, vel_saved, octave_idx, muted, preset_idx) = storage.query(|s| {
        (
            s.chord_saved,
            s.spread_saved,
            s.vel_saved,
            s.octave_idx,
            s.muted,
            s.preset_idx,
        )
    });

    let muted_glob = app.make_global(muted);
    let octave_glob = app.make_global(octave_idx);
    let chord_glob = app.make_global(chord_saved);
    let spread_glob = app.make_global(spread_saved);
    let vel_glob = app.make_global(vel_saved);
    let latch_glob = app.make_global(LatchLayer::Main);
    let long_press_fired = app.make_global(false);
    let panic_flag = app.make_global(false);
    let revoice_flag = app.make_global(true);
    let root_glob = app.make_global(None::<u8>);
    let root_vel_glob = app.make_global(4095u16);
    let button_duck = app.make_global(0u16);
    // Strum is live-settable by presets; the param is only the power-on value.
    let strum_glob = app.make_global(strum_ms);
    let preset_glob = app.make_global(preset_idx);
    let preset_flash = app.make_global(0u16);

    // Restore the stepped preset so spread/octave/strum survive a power cycle.
    if let Some(&(spread_step, oct, strum)) = PRESETS.get(preset_idx as usize) {
        spread_glob.set(spread_value(spread_step));
        octave_glob.set(oct);
        strum_glob.set(strum);
    }

    let apply_preset = |idx: usize| {
        let (spread_step, oct, strum) = PRESETS[idx.min(NUM_PRESETS - 1)];
        let spread = spread_value(spread_step);
        spread_glob.set(spread);
        octave_glob.set(oct);
        strum_glob.set(strum);
        preset_glob.set(idx as u8);
        preset_flash.set(PRESET_FLASH_MS);
        storage.modify_and_save(|s| {
            s.spread_saved = spread;
            s.octave_idx = oct;
            s.preset_idx = idx as u8;
        });
        revoice_flag.set(true);
    };

    if muted {
        leds.set(0, Led::Button, Color::Red, Brightness::Low);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let engine = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut sounding: Vec<u8, MAX_VOICES> = Vec::new();
        let mut held: Vec<u8, MAX_HELD> = Vec::new();
        let mut cv_voice = CvVoice::Idle;
        let mut cv_stable_note: Option<u8> = None;
        let mut cv_stable_count = 0u16;
        let mut cv_hist = [0u16; CV_HIST];
        let mut cv_hist_i = 0usize;
        let mut cv_hist_fill = 0usize;
        let mut cv_slew_ms = 0u16;
        // True after ADC motion — next stable lock may sound (vs quiet float park).
        let mut cv_saw_motion = false;
        // Countdown after plug/unplug noise — block note-starts until settled.
        let mut cv_blank_ms = 0u16;
        // CV→MIDI ms pulse (None / 0 = sustain or clock-gated).
        let mut cv_gate_left = 0u16;
        // Clock-relative gate: (start_tick, length_ticks).
        let mut cv_gate_deadline: Option<(u64, u64)> = None;
        // Previous frame was at rest (near 0V). Leaving rest is a trigger:
        // the rest branch clears cv_hist, so the 0V→pitch jump would
        // otherwise never register as motion and Idle would park in Armed.
        // Starts false so an unpatched mid-rail float at boot is not a
        // fake rest→pitch trigger (it parks in Armed as before).
        let mut cv_was_rest = false;
        let mut strum_pending: StrumQueue = Vec::new();

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

            // ── Latch layers ──────────────────────────────────────────────
            let latch_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            latch_glob.set(latch_layer);

            let chord_val = chord_glob.get();
            let spread_val = spread_glob.get();
            let vel_val = vel_glob.get();
            let target = match latch_layer {
                LatchLayer::Main => chord_val,
                LatchLayer::Alt => spread_val,
                LatchLayer::Third => vel_val,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target) {
                match latch_layer {
                    LatchLayer::Main => {
                        chord_glob.set(new_value);
                        storage.modify(|s| s.chord_saved = new_value);
                        revoice_flag.set(true);
                    }
                    LatchLayer::Alt => {
                        spread_glob.set(new_value);
                        // Hand-editing spread leaves the preset behind.
                        preset_glob.set(NUM_PRESETS as u8);
                        storage.modify(|s| {
                            s.spread_saved = new_value;
                            s.preset_idx = NUM_PRESETS as u8;
                        });
                        revoice_flag.set(true);
                    }
                    LatchLayer::Third => {
                        vel_glob.set(new_value);
                        storage.modify(|s| s.vel_saved = new_value);
                    }
                }
            }

            // ── Panic ─────────────────────────────────────────────────────
            if panic_flag.get() {
                panic_flag.set(false);
                all_notes_off(&midi, &mut sounding, &mut strum_pending).await;
                held.clear();
                root_glob.set(None);
                cv_voice = CvVoice::Idle;
                cv_stable_note = None;
                cv_stable_count = 0;
                cv_hist_fill = 0;
                cv_hist_i = 0;
                cv_slew_ms = 0;
                cv_saw_motion = false;
                cv_blank_ms = 0;
                cv_gate_left = 0;
                cv_gate_deadline = None;
                if let Some(ref jack) = out_jack {
                    jack.set_value(0);
                }
            }

            // ── MIDI input (MIDI→MIDI / MIDI→CV) ──────────────────────────
            if let Some(msg) = midi_msg {
                match msg {
                    MidiMessage::NoteOn { key, vel } if vel > 0 => {
                        let key_n = key.as_int();
                        // Same In/Out CH + host/DIN thru echoes our chord tones back.
                        // Those must not enter the held stack or the gate never falls.
                        if sounding.contains(&key_n) && !held.contains(&key_n) {
                            // drop echo
                        } else {
                            let vel12 = ((vel.as_int() as u32 * 4095) / 127) as u16;
                            held_note_on(&mut held, key_n);
                            root_glob.set(Some(key_n));
                            root_vel_glob.set(vel12);
                            revoice_flag.set(true);
                            button_duck.set(BUTTON_DUCK_MS);
                        }
                    }
                    MidiMessage::NoteOn { key, vel } if vel == 0 => {
                        let key_n = key.as_int();
                        handle_midi_note_off(
                            &midi,
                            &mut held,
                            &mut sounding,
                            &mut strum_pending,
                            &root_glob,
                            &revoice_flag,
                            out_jack.as_ref(),
                            key_n,
                        )
                        .await;
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let key_n = key.as_int();
                        handle_midi_note_off(
                            &midi,
                            &mut held,
                            &mut sounding,
                            &mut strum_pending,
                            &root_glob,
                            &revoice_flag,
                            out_jack.as_ref(),
                            key_n,
                        )
                        .await;
                    }
                    _ => {}
                }
            }

            // ── CV→MIDI: park quiet float; play after motion then lock ────
            if let Some(ref jack) = in_jack {
                let raw = jack.get_value();

                // Note Fader release / unplugged-to-0V — never treat as a root.
                if raw < CV_REST_COUNTS {
                    if root_glob.get().is_some() || !sounding.is_empty() {
                        all_notes_off(&midi, &mut sounding, &mut strum_pending).await;
                        root_glob.set(None);
                    }
                    cv_voice = CvVoice::Idle;
                    cv_saw_motion = false;
                    cv_stable_note = None;
                    cv_stable_count = 0;
                    cv_blank_ms = 0;
                    cv_slew_ms = 0;
                    cv_gate_left = 0;
                    cv_gate_deadline = None;
                    cv_hist_fill = 0;
                    cv_hist_i = 0;
                    cv_was_rest = true;
                } else {
                    if cv_was_rest {
                        // Rest → pitch = Note Fader press / patch at level.
                        // Count as motion so the first stable lock plays.
                        cv_was_rest = false;
                        cv_saw_motion = true;
                    }
                    cv_hist[cv_hist_i] = raw;
                    cv_hist_i = (cv_hist_i + 1) % CV_HIST;
                    if cv_hist_fill < CV_HIST {
                        cv_hist_fill += 1;
                    }

                    let pp = if cv_hist_fill >= CV_HIST {
                        let (mn, mx) = cv_hist
                            .iter()
                            .fold((u16::MAX, 0u16), |(a, b), &v| (a.min(v), b.max(v)));
                        mx.saturating_sub(mn)
                    } else {
                        0
                    };
                    let motion = pp > CV_MOTION_PP;
                    let retrig = pp > CV_RETRIG_PP;
                    if motion || retrig {
                        cv_saw_motion = true;
                        // Contact bounce / cable insert dumps random ADC — mute new notes.
                        cv_blank_ms = CV_SETTLE_BLANK_MS;
                    } else if cv_blank_ms > 0 {
                        cv_blank_ms = cv_blank_ms.saturating_sub(1);
                    }
                    // Settled = quiet window finished and no current motion.
                    let settled = cv_blank_ms == 0 && !motion;

                    let pitched = quantizer.get_quantized_note(raw).await;
                    let note = midi_u8(pitched.as_midi());
                    if cv_stable_note == Some(note) {
                        cv_stable_count = cv_stable_count.saturating_add(1);
                    } else {
                        cv_stable_note = Some(note);
                        cv_stable_count = 1;
                    }
                    let locked = cv_stable_count >= CV_STABLE_MS;

                    let mut start_note: Option<u8> = None;

                    match cv_voice {
                        CvVoice::Idle => {
                            cv_slew_ms = 0;
                            if settled && locked {
                                if cv_saw_motion {
                                    // Patch / stepped CV / post-slew settle → sound.
                                    cv_voice = CvVoice::Playing { note };
                                    start_note = Some(note);
                                    cv_saw_motion = false;
                                } else {
                                    // Quiet from boot — park unpatched mid-rail.
                                    cv_voice = CvVoice::Armed { note };
                                }
                            }
                        }
                        CvVoice::Armed { note: armed } => {
                            cv_slew_ms = 0;
                            if motion {
                                cv_voice = CvVoice::Idle;
                            } else if settled && locked && note != armed {
                                cv_voice = CvVoice::Playing { note };
                                start_note = Some(note);
                            }
                        }
                        CvVoice::Playing { note: playing } => {
                            cv_slew_ms = 0;
                            if retrig {
                                // Real slew / unplug — ignore small ADC chatter on held CV.
                                cv_voice = CvVoice::Slewing;
                                cv_slew_ms = 1;
                            } else if settled && locked && note != playing {
                                cv_voice = CvVoice::Playing { note };
                                start_note = Some(note);
                            }
                        }
                        CvVoice::Slewing => {
                            if settled && locked {
                                // Brief slew = stepped CV between notes → resume.
                                // Long slew = plug/unplug chaos → park silent (Armed).
                                if cv_slew_ms <= 40 {
                                    cv_voice = CvVoice::Playing { note };
                                    start_note = Some(note);
                                } else {
                                    cv_voice = CvVoice::Armed { note };
                                }
                                cv_slew_ms = 0;
                                cv_saw_motion = false;
                            } else {
                                cv_slew_ms = cv_slew_ms.saturating_add(1);
                                if cv_slew_ms > CV_UNPLUG_MS {
                                    cv_voice = CvVoice::Idle;
                                    cv_slew_ms = 0;
                                    cv_saw_motion = false;
                                }
                            }
                        }
                    }

                    // Gate follows stable pitch: end chord as soon as we leave Playing
                    // (no ringout while slewing / unplugging / blanking).
                    if !matches!(cv_voice, CvVoice::Playing { .. })
                        && start_note.is_none()
                        && (root_glob.get().is_some() || !sounding.is_empty())
                    {
                        all_notes_off(&midi, &mut sounding, &mut strum_pending).await;
                        root_glob.set(None);
                        cv_gate_left = 0;
                        cv_gate_deadline = None;
                    }
                    if let Some(n) = start_note {
                        root_glob.set(Some(n));
                        root_vel_glob.set(4095);
                        revoice_flag.set(true);
                        button_duck.set(BUTTON_DUCK_MS);
                        cv_gate_left = 0;
                        cv_gate_deadline = None;
                        if let Some(ms) = gate_ms {
                            cv_gate_left = ms;
                        } else if let Some(len) = gate_ticks {
                            cv_gate_deadline = Some((harmonica_ticks(), len));
                        }
                    } else if matches!(cv_voice, CvVoice::Playing { .. }) {
                        let mut gate_done = false;
                    if let Some(_ms) = gate_ms {
                        if cv_gate_left > 0 {
                            cv_gate_left = cv_gate_left.saturating_sub(1);
                            gate_done = cv_gate_left == 0;
                        }
                    } else if let Some((start, len)) = cv_gate_deadline {
                            if harmonica_ticks().wrapping_sub(start) >= len {
                                gate_done = true;
                                cv_gate_deadline = None;
                            }
                        }
                        if gate_done {
                            all_notes_off(&midi, &mut sounding, &mut strum_pending).await;
                            root_glob.set(None);
                            // Park silent — same pitch needs motion or a new note
                            // (no auto-pulse on float / held CV).
                            cv_voice = CvVoice::Armed { note };
                            cv_saw_motion = false;
                        }
                    }
                } // end non-rest CV
            }

            // ── Live revoice ──────────────────────────────────────────────
            if revoice_flag.get() {
                revoice_flag.set(false);
                if let Some(root) = root_glob.get() {
                    let chord_type = value_to_index(chord_glob.get(), NUM_CHORD_TYPES);
                    let new_voices = build_voices(
                        &quantizer,
                        VoiceParams {
                            root,
                            chord_type,
                            spread: spread_glob.get(),
                            octave_idx: octave_glob.get(),
                            muted: muted_glob.get(),
                            range,
                            vpo,
                            bypass,
                        },
                    )
                    .await;

                    // Always MIDI-out (incl. MIDI→CV) so Scopepunk can monitor;
                    // jack still follows sounding below when Out mode is CV.
                    revoice(
                        &midi,
                        &sounding,
                        &new_voices,
                        Some(root),
                        root_vel_glob.get(),
                        vel_glob.get(),
                        strum_glob.get(),
                        &mut strum_pending,
                    )
                    .await;
                    sounding = new_voices;
                } else if !sounding.is_empty() {
                    all_notes_off(&midi, &mut sounding, &mut strum_pending).await;
                }
            }

            if !strum_pending.is_empty() {
                drain_strum(&midi, &mut strum_pending).await;
            }

            // ── MIDI→CV jack ──────────────────────────────────────────────
            if let Some(ref jack) = out_jack {
                if muted_glob.get() {
                    jack.set_value(0);
                } else if let Some(&top) = sounding.iter().max() {
                    jack.set_value(midi_to_pitch(top).as_counts(range, vpo));
                } else {
                    jack.set_value(0);
                }
            }

            // ── LEDs ──────────────────────────────────────────────────────
            let flash = preset_flash.get();
            if flash > 0 {
                preset_flash.set(flash - 1);
                // One solid blink: IR→UV by preset index on all three LEDs.
                // Map idx so preset 0 is still a bright red (not off-scale).
                let idx = preset_glob.get().min(NUM_PRESETS as u8 - 1) as u32;
                let faderish = ((idx * 4095) / (NUM_PRESETS as u32 - 1).max(1)) as u16;
                let color = spectrum_color(faderish);
                leds.set(0, Led::Top, color, Brightness::High);
                leds.set(0, Led::Bottom, color, Brightness::High);
                leds.set(0, Led::Button, color, Brightness::High);
            } else {
                let (fader_val, color) = match latch_layer {
                    LatchLayer::Main => {
                        let v = chord_glob.get();
                        (v, spectrum_color(v))
                    }
                    LatchLayer::Alt => {
                        let v = spread_glob.get();
                        (v, spectrum_color(v))
                    }
                    LatchLayer::Third => {
                        let v = vel_glob.get();
                        (v, spectrum_color(v))
                    }
                };

                if muted_glob.get() {
                    leds.set(0, Led::Button, Color::Red, Brightness::Low);
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                } else {
                    let duck = button_duck.get();
                    let btn = if duck > 0 {
                        40u8
                    } else if sounding.iter().any(|&n| n > 0) {
                        200u8
                    } else {
                        (fader_val / 16).max(36) as u8
                    };
                    paint_fader_meters(&leds, color, fader_val, btn);
                    // Pitch cue on Top when Main + sounding (override meter briefly).
                    if latch_layer == LatchLayer::Main {
                        if let Some(&top) = sounding.iter().max() {
                            let pitch_f = (u16::from(top).saturating_mul(32)).min(4095);
                            leds.set(
                                0,
                                Led::Top,
                                spectrum_color(pitch_f),
                                Brightness::Custom((pitch_f / 16).max(20) as u8),
                            );
                        }
                    }
                }
            }

            let duck = button_duck.get();
            if duck > 0 {
                button_duck.set(duck.saturating_sub(1));
            }
        }
    };

    let button_handler = async {
        // Mute = short tap without fader move (Third = Button+Fader velocity).
        // Mute silences all output (root + harmony); unmute restores if a root is held.
        const FADER_MOVE_DEADBAND: u16 = 48;
        loop {
            let shift = buttons.wait_for_down(0).await;
            if shift {
                // Alt+tap → harmony octave; Alt+hold 500ms → next voicing preset.
                // Detected here with a timer so we don't depend on a second
                // long-press subscriber still seeing Shift held.
                match select(buttons.wait_for_up(0), app.delay_millis(ALT_LONG_MS)).await {
                    Either::First(_) => {
                        let next = match octave_glob.get() {
                            0 => 1,
                            1 => 2,
                            _ => 0,
                        };
                        octave_glob.set(next);
                        storage.modify_and_save(|s| s.octave_idx = next);
                        revoice_flag.set(true);
                    }
                    Either::Second(_) => {
                        let next = match preset_glob.get() as usize {
                            i if i + 1 < NUM_PRESETS => i + 1,
                            _ => 0,
                        };
                        apply_preset(next);
                        buttons.wait_for_up(0).await;
                    }
                }
            } else {
                long_press_fired.set(false);
                let fader_at_down = fader.get_value();
                buttons.wait_for_up(0).await;
                let moved = fader.get_value().abs_diff(fader_at_down) >= FADER_MOVE_DEADBAND;
                if !moved {
                    if long_press_fired.get() {
                        panic_flag.set(true);
                    } else {
                        let muted = !muted_glob.get();
                        muted_glob.set(muted);
                        storage.modify_and_save(|s| s.muted = muted);
                        revoice_flag.set(true);
                        if muted {
                            leds.set(0, Led::Button, Color::Red, Brightness::Low);
                        } else {
                            leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                        }
                    }
                }
            }
        }
    };

    let long_press_handler = async {
        loop {
            // Non-Alt only — panic runs on release if the fader wasn't moved
            // (so Button+Fader velocity edits never wipe notes mid-hold).
            // Alt+long is handled in button_handler above.
            let (_, shift) = buttons.wait_for_any_long_press().await;
            if !shift {
                long_press_fired.set(true);
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (chord, spread, vel, oct, muted, preset) = storage.query(|s| {
                        (
                            s.chord_saved,
                            s.spread_saved,
                            s.vel_saved,
                            s.octave_idx,
                            s.muted,
                            s.preset_idx,
                        )
                    });
                    chord_glob.set(chord);
                    spread_glob.set(spread);
                    vel_glob.set(vel);
                    octave_glob.set(oct);
                    muted_glob.set(muted);
                    preset_glob.set(preset);
                    strum_glob.set(
                        PRESETS
                            .get(preset as usize)
                            .map_or(strum_ms, |&(_, _, strum)| strum),
                    );
                    revoice_flag.set(true);
                    if muted {
                        leds.set(0, Led::Button, Color::Red, Brightness::Low);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join(
        join4(engine, button_handler, long_press_handler, scene_handler),
        clock_tick_drain(),
    )
    .await;
}
