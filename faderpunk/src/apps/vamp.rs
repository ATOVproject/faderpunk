use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use midly::num::u7;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate_bipolar, split_unsigned_value, value_to_index, value_to_resolution},
    AppIcon, Brightness, ClockDivision, Color, Config, Key, MidiCc, MidiChannel, MidiNote, MidiOut,
    Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Die, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 13;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const VAMP_CAP: usize = 32;
const RING_CAP: usize = 64;
const SOUNDING_CAP: usize = 8;
const TICKS_PER_BAR: u32 = 96;
const NUM_GENRES: usize = 8;
const NUM_DEGREES: usize = 7;
/// Shift+Fader index for the Capture clip (after the 8 genres).
const CAPTURE_SLOT: usize = NUM_GENRES;
const CAPTURE_COLOR: Color = Color::Rose;

/// Same genre set as Grooves (oldest → newest).
const GENRE_NAMES: &[&str] = &[
    "Dub",
    "Disco",
    "Hip-Hop",
    "House",
    "Techno",
    "Trip-Hop",
    "UK Garage",
    "Dubstep",
];

/// Match Grooves so Shift+Fader genre pick is recognizable across apps.
const GENRE_COLORS: [Color; NUM_GENRES] = [
    Color::Orange, // Dub
    Color::Yellow, // Disco
    Color::Red,    // Hip-Hop
    Color::Pink,   // House
    Color::Cyan,   // Techno
    Color::Violet, // Trip-Hop
    Color::Green,  // UK Garage
    Color::Blue,   // Dubstep
];

const CAPTURE_NAMES: &[&str] = &["16 bars", "8 bars", "4 bars", "2 bars"];
/// Bars for each Capture length enum index.
const CAPTURE_BARS: [u32; 4] = [16, 8, 4, 2];

const SCALE_NAMES: &[&str] = &[
    "Ionian",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Aeolian",
    "Locrian",
    "Pent Maj",
    "Pent Min",
];

const VOICING_NAMES: &[&str] = &["Triads", "7ths", "9ths", "Spread"];
const AUTO_STYLE_NAMES: &[&str] = &["Repeat", "Meander"];
const START_MODE_NAMES: &[&str] = &["Perform", "Auto"];

const DEST_MACRO: usize = 0;
const DEST_PANIC: usize = 1;
const DEST_COUNT: usize = 2;
const TRIG_HIGH: u16 = 2458;

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 100) as u32 * 4095) / 100) as u16
}

fn mod_u16(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

/// Auto step length in ticks (24 PPQN): 8/1 … 1/8-ish.
/// Slow end first so the fader spends more travel on musical chord rates.
const CLOCK_RESOLUTIONS: &[u16] = &[
    768, // 8/1  (8 bars)
    384, // 4/1
    192, // 2/1
    96,  // 1/1
    48,  // 1/2
    24,  // 1/4
    16,  // 1/6
    12,  // 1/8
    8,   // 1/12
    6,   // 1/16
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "MIDI Vamp",
    "Chord progressions — perform, capture, or auto vamp",
    Color::Violet,
    AppIcon::NoteGrid,
)
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote {
    name: "Root",
})
.add_param(Param::Enum {
    name: "Scale",
    variants: SCALE_NAMES,
})
.add_param(Param::Enum {
    name: "Genre",
    variants: GENRE_NAMES,
})
.add_param(Param::Enum {
    name: "Voicing",
    variants: VOICING_NAMES,
})
.add_param(Param::i32 {
    name: "Velocity",
    min: 1,
    max: 127,
})
.add_param(Param::Enum {
    name: "Auto style",
    variants: AUTO_STYLE_NAMES,
})
.add_param(Param::Enum {
    name: "Start mode",
    variants: START_MODE_NAMES,
})
.add_param(Param::Enum {
    name: "Capture length",
    variants: CAPTURE_NAMES,
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
.add_param(Param::Enum {
    name: "CV Dest",
    variants: &["Macro", "Panic"],
})
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 100,
});

pub struct Params {
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    root: MidiNote,
    scale: usize,
    genre: usize,
    voicing: usize,
    velocity: i32,
    auto_style: usize,
    start_mode: usize,
    capture_len: usize,
    color: Color,
    cv_dest: usize,
    cv_att: i32,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < 11 {
            return None;
        }
        let (cv_dest, cv_att) = if values.len() >= PARAMS {
            (
                usize::from_value(values[11]).min(DEST_COUNT - 1),
                i32::from_value(values[12]).clamp(0, 100),
            )
        } else {
            (DEST_MACRO, 100)
        };
        Some(Self {
            midi_out: MidiOut::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            root: MidiNote::from_value(values[2]),
            scale: usize::from_value(values[3]),
            genre: usize::from_value(values[4]).min(NUM_GENRES - 1),
            voicing: usize::from_value(values[5]),
            velocity: i32::from_value(values[6]).clamp(1, 127),
            auto_style: usize::from_value(values[7]),
            start_mode: usize::from_value(values[8]),
            capture_len: usize::from_value(values[9]).min(CAPTURE_BARS.len() - 1),
            color: Color::from_value(values[10]),
            cv_dest,
            cv_att,
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.root.into()).unwrap();
        vec.push(self.scale.into()).unwrap();
        vec.push(self.genre.into()).unwrap();
        vec.push(self.voicing.into()).unwrap();
        vec.push(self.velocity.into()).unwrap();
        vec.push(self.auto_style.into()).unwrap();
        vec.push(self.start_mode.into()).unwrap();
        vec.push(self.capture_len.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.cv_dest.into()).unwrap();
        vec.push(self.cv_att.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Active vamp degrees (I=0 … vii=6) for Perform scrub / genre Auto.
    slots: [u8; VAMP_CAP],
    slot_count: u8,
    genre: u8,
    scrub: u16,
    tension: u16,
    div_fader: u16,
    mode_auto: bool,
    meander: bool,
    auto_running: bool,
    /// Timed capture clip (relative on/dur within `clip_bars` bars).
    clip_active: bool,
    clip_len: u8,
    clip_bars: u8,
    clip_deg: [u8; VAMP_CAP],
    clip_on: [u16; VAMP_CAP],
    clip_dur: [u16; VAMP_CAP],
}

impl Default for Storage {
    fn default() -> Self {
        let genre = GenrePreset::get(3); // House
        let mut slots = [0u8; VAMP_CAP];
        let n = genre.progression.len().min(VAMP_CAP);
        slots[..n].copy_from_slice(&genre.progression[..n]);
        Self {
            slots,
            slot_count: n as u8,
            genre: 3,
            scrub: 0,
            tension: 2048,
            div_fader: 1536, // ~1/1 (96 ticks) with CLOCK_RESOLUTIONS above
            mode_auto: false,
            meander: false,
            auto_running: true,
            clip_active: false,
            clip_len: 0,
            clip_bars: 8,
            clip_deg: [0u8; VAMP_CAP],
            clip_on: [0u16; VAMP_CAP],
            clip_dur: [0u16; VAMP_CAP],
        }
    }
}

impl AppStorage for Storage {}

#[derive(Clone, Copy)]
struct GenrePreset {
    /// Fixed Repeat progression (degrees).
    progression: &'static [u8],
    /// Markov weights from → to (rows sum arbitrary; sampled by weight).
    markov: &'static [[u8; NUM_DEGREES]; NUM_DEGREES],
}

impl GenrePreset {
    fn get(index: usize) -> Self {
        GENRES[index.min(NUM_GENRES - 1)]
    }
}

/// Genre chord DNA (degrees 0–6). Markov biases genre-typical moves.
const GENRES: [GenrePreset; NUM_GENRES] = [
    // Dub — i–IV–i–V
    GenrePreset {
        progression: &[0, 3, 0, 4, 0, 3, 0, 4],
        markov: &[
            [4, 1, 1, 6, 5, 2, 2],
            [3, 2, 1, 2, 3, 2, 1],
            [2, 2, 2, 2, 2, 3, 2],
            [5, 1, 1, 2, 4, 2, 1],
            [6, 1, 1, 3, 2, 2, 2],
            [3, 2, 2, 2, 2, 2, 3],
            [4, 1, 1, 2, 3, 2, 2],
        ],
    },
    // Disco — I–vi–IV–V
    GenrePreset {
        progression: &[0, 5, 3, 4, 0, 5, 3, 4],
        markov: &[
            [2, 1, 1, 4, 5, 6, 1],
            [2, 2, 2, 2, 3, 2, 2],
            [2, 2, 2, 3, 2, 3, 2],
            [3, 1, 1, 2, 6, 2, 1],
            [6, 1, 1, 3, 2, 2, 1],
            [3, 1, 2, 5, 3, 2, 1],
            [4, 1, 1, 2, 4, 2, 1],
        ],
    },
    // Hip-Hop — i–VI–III–VII
    GenrePreset {
        progression: &[0, 5, 2, 6, 0, 5, 2, 6],
        markov: &[
            [3, 1, 4, 2, 2, 5, 4],
            [2, 2, 2, 2, 2, 3, 2],
            [3, 1, 2, 2, 2, 4, 3],
            [3, 2, 2, 2, 3, 2, 2],
            [4, 1, 2, 2, 2, 3, 3],
            [4, 1, 3, 2, 2, 2, 3],
            [5, 1, 2, 2, 2, 3, 2],
        ],
    },
    // House — i–VII–VI–VII
    GenrePreset {
        progression: &[0, 6, 5, 6, 0, 6, 5, 6],
        markov: &[
            [3, 1, 1, 2, 2, 4, 6],
            [2, 2, 2, 2, 2, 3, 2],
            [2, 2, 2, 2, 2, 3, 3],
            [3, 1, 1, 2, 3, 3, 3],
            [3, 1, 1, 2, 2, 3, 4],
            [4, 1, 1, 2, 2, 2, 5],
            [5, 1, 1, 2, 2, 4, 2],
        ],
    },
    // Techno — static/minimal
    GenrePreset {
        progression: &[0, 0, 0, 4, 0, 0, 0, 4],
        markov: &[
            [8, 1, 1, 2, 4, 2, 3],
            [4, 2, 1, 1, 2, 1, 1],
            [3, 1, 2, 1, 2, 1, 2],
            [4, 1, 1, 2, 3, 1, 2],
            [6, 1, 1, 2, 2, 2, 2],
            [4, 1, 1, 2, 2, 2, 3],
            [5, 1, 1, 1, 3, 2, 2],
        ],
    },
    // Trip-Hop — i–VII–VI–v
    GenrePreset {
        progression: &[0, 6, 5, 4, 0, 6, 5, 4],
        markov: &[
            [3, 1, 2, 2, 4, 4, 5],
            [2, 2, 2, 2, 2, 3, 2],
            [2, 2, 2, 2, 3, 3, 2],
            [3, 1, 2, 2, 3, 3, 2],
            [4, 1, 1, 2, 2, 3, 3],
            [4, 1, 2, 2, 3, 2, 4],
            [5, 1, 1, 2, 3, 4, 2],
        ],
    },
    // UK Garage — i–III–VI–VII
    GenrePreset {
        progression: &[0, 2, 5, 6, 0, 2, 5, 6],
        markov: &[
            [3, 1, 5, 2, 2, 4, 4],
            [2, 2, 3, 2, 2, 3, 2],
            [3, 2, 2, 2, 2, 4, 3],
            [3, 1, 2, 2, 3, 2, 2],
            [3, 1, 2, 2, 2, 3, 3],
            [4, 1, 3, 2, 2, 2, 4],
            [5, 1, 2, 2, 2, 3, 2],
        ],
    },
    // Dubstep — i–i–VI–VII
    GenrePreset {
        progression: &[0, 0, 5, 6, 0, 0, 5, 6],
        markov: &[
            [6, 1, 1, 2, 2, 5, 5],
            [3, 2, 1, 1, 2, 2, 2],
            [3, 1, 2, 1, 2, 2, 2],
            [3, 1, 1, 2, 3, 2, 2],
            [4, 1, 1, 2, 2, 3, 3],
            [4, 1, 1, 2, 2, 2, 5],
            [5, 1, 1, 1, 3, 4, 2],
        ],
    },
];

fn midi_u8(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn scale_index_to_key(index: usize) -> Key {
    match index {
        1 => Key::Dorian,
        2 => Key::Phrygian,
        3 => Key::Lydian,
        4 => Key::Mixolydian,
        5 => Key::Aeolian,
        6 => Key::Locrian,
        7 => Key::PentatonicMaj,
        8 => Key::PentatonicMin,
        _ => Key::Ionian,
    }
}

fn scale_offsets(key: Key) -> Vec<u8, 12> {
    let mask = key.as_u16_key();
    let mut notes = Vec::new();
    for i in 0..12u8 {
        if (mask >> (11 - i)) & 1 != 0 {
            let _ = notes.push(i);
        }
    }
    if notes.is_empty() {
        let _ = notes.push(0);
    }
    notes
}

fn velocity_12bit(vel_7: i32) -> u16 {
    let v = vel_7.clamp(1, 127) as u32;
    ((v * 4095) / 127) as u16
}

/// Build MIDI note numbers for a diatonic chord on `degree` (0=I … 6=vii).
fn build_chord(root_midi: u8, key: Key, degree: u8, voicing: usize) -> Vec<u8, SOUNDING_CAP> {
    let scale = scale_offsets(key);
    let n = scale.len();
    let ext: &[usize] = match voicing {
        1 => &[0, 2, 4, 6],
        2 => &[0, 2, 4, 6, 8],
        _ => &[0, 2, 4],
    };

    let mut out: Vec<u8, SOUNDING_CAP> = Vec::new();
    let base_degree = degree as usize % NUM_DEGREES;

    for (vi, &steps) in ext.iter().enumerate() {
        let d = base_degree + steps;
        let oct = (d / n) as i16;
        let idx = d % n;
        let semis = oct * 12 + scale[idx] as i16;
        // Degree 0 / steps 0 → scale[0] which is 0 relative to tonic → root_midi
        let mut note = (root_midi as i16 + semis - scale[0] as i16).clamp(0, 127) as u8;
        if voicing == 3 && vi > 0 {
            note = (note as u16).saturating_add(12u16 * vi as u16).min(127) as u8;
        }
        let _ = out.push(note);
    }
    out
}

fn pick_markov(from: u8, weights: &[[u8; NUM_DEGREES]; NUM_DEGREES], die: &Die) -> u8 {
    let row = &weights[(from as usize) % NUM_DEGREES];
    let total: u32 = row.iter().map(|&w| w as u32).sum();
    if total == 0 {
        return from;
    }
    let mut pick = (die.roll() as u32 * total) / 4096;
    for (i, &w) in row.iter().enumerate() {
        if pick < w as u32 {
            return i as u8;
        }
        pick = pick.saturating_sub(w as u32);
    }
    (NUM_DEGREES - 1) as u8
}

fn load_genre_into(slots: &mut [u8; VAMP_CAP], genre: usize) -> u8 {
    let g = GenrePreset::get(genre);
    let n = g.progression.len().min(VAMP_CAP);
    slots[..n].copy_from_slice(&g.progression[..n]);
    for s in slots.iter_mut().skip(n) {
        *s = 0;
    }
    n as u8
}

/// Build a relative timed clip from the always-record ring (last `bars` bars).
#[allow(clippy::too_many_arguments)]
fn build_clip_from_ring(
    bars: u32,
    now: u32,
    ring_len: usize,
    ring_write: usize,
    deg: &[u8; RING_CAP],
    ons: &[u32; RING_CAP],
    offs: &[u32; RING_CAP],
    open_idx: u8,
) -> (u8, [u8; VAMP_CAP], [u16; VAMP_CAP], [u16; VAMP_CAP]) {
    let window = bars.saturating_mul(TICKS_PER_BAR);
    let clip_start = now.wrapping_sub(window);
    let mut out_deg = [0u8; VAMP_CAP];
    let mut out_on = [0u16; VAMP_CAP];
    let mut out_dur = [0u16; VAMP_CAP];
    let mut n = 0usize;

    if ring_len == 0 || window == 0 {
        return (0, out_deg, out_on, out_dur);
    }

    for i in 0..ring_len {
        let idx = if ring_len < RING_CAP {
            i
        } else {
            (ring_write + i) % RING_CAP
        };
        let on = ons[idx];
        let age = now.wrapping_sub(on);
        if age >= window || age >= (u32::MAX / 2) {
            continue;
        }
        let mut off = offs[idx];
        if off == 0 || (open_idx as usize == idx) {
            off = now;
        }
        // Skip malformed
        let dur_abs = off.wrapping_sub(on);
        if dur_abs == 0 || dur_abs >= (u32::MAX / 2) {
            continue;
        }
        let rel_on = on.wrapping_sub(clip_start);
        if rel_on >= window {
            continue;
        }
        let mut dur = dur_abs.min(window.saturating_sub(rel_on)).max(1);
        // Keep note-off inside one loop iteration.
        if rel_on + dur > window {
            dur = window - rel_on;
        }
        if dur == 0 || n >= VAMP_CAP {
            continue;
        }
        out_deg[n] = deg[idx];
        out_on[n] = rel_on as u16;
        out_dur[n] = dur as u16;
        n += 1;
    }

    (n as u8, out_deg, out_on, out_dur)
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_out: MidiOut::default(),
            midi_channel: MidiChannel::default(),
            root: MidiNote::from(48),
            scale: 5, // Aeolian
            genre: 3, // House
            voicing: 0,
            velocity: 100,
            auto_style: 0,
            start_mode: 0,
            capture_len: 1, // 8 bars
            color: Color::Violet,
            cv_dest: DEST_MACRO,
            cv_att: 100,
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
        midi_out_cfg,
        midi_chan,
        root,
        scale_idx,
        genre_param,
        voicing,
        velocity,
        auto_style,
        start_mode,
        capture_len,
        led_color,
        cv_dest,
        cv_att,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.root,
            p.scale,
            p.genre.min(NUM_GENRES - 1),
            p.voicing,
            p.velocity,
            p.auto_style,
            p.start_mode,
            p.capture_len.min(CAPTURE_BARS.len() - 1),
            p.color,
            p.cv_dest.min(DEST_COUNT - 1),
            att_from_pct(p.cv_att),
        )
    });

    let root_midi = midi_u8(root);
    let key = scale_index_to_key(scale_idx);
    let vel12 = velocity_12bit(velocity);

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let die = app.use_die();
    let midi = app.use_midi_output(midi_out_cfg, midi_chan, false);
    let in_jack = app.make_in_jack(0, Range::_Neg5_5V).await;

    let glob_latch = app.make_global(LatchLayer::Main);
    let glob_mode_auto = app.make_global(false);
    let glob_meander = app.make_global(false);
    let glob_auto_running = app.make_global(false);
    let glob_scrub = app.make_global(0u16);
    let glob_tension = app.make_global(2048u16);
    let glob_div = app.make_global(24u32);
    let glob_genre = app.make_global(genre_param);
    let glob_slot_count = app.make_global(8u8);
    let glob_slot_idx = app.make_global(0u8);
    let glob_degree = app.make_global(0u8);
    let glob_activity = app.make_global(0u8);
    let glob_capture_flash = app.make_global(0u8);
    let long_press_fired = app.make_global(false);
    let panic_flag = app.make_global(false);
    let glob_cv_val = app.make_global(2047u16);
    let chord_held = app.make_global(false);
    let ring_write = app.make_global(0u8);
    let ring_len = app.make_global(0u8);
    let ring_open = app.make_global(255u8); // 255 = no open note-on
    // Clock watch → voice engine (never await MIDI inside the clock subscriber).
    let pending_play = app.make_global(false);
    let pending_degree = app.make_global(0u8);
    let pending_release = app.make_global(false);
    let pending_record = app.make_global(false);

    // Shared vamp slots + timed ring + capture clip.
    let slots_cell = app.make_global([0u8; VAMP_CAP]);
    let ring_deg = app.make_global([0u8; RING_CAP]);
    let ring_on = app.make_global([0u32; RING_CAP]);
    let ring_off = app.make_global([0u32; RING_CAP]);
    let clip_deg_cell = app.make_global([0u8; VAMP_CAP]);
    let clip_on_cell = app.make_global([0u16; VAMP_CAP]);
    let clip_dur_cell = app.make_global([0u16; VAMP_CAP]);
    let glob_clip_active = app.make_global(false);
    let glob_clip_armed = app.make_global(false);
    let glob_clip_origin = app.make_global(0u32);
    let glob_clip_len = app.make_global(0u8);
    let glob_clip_bars = app.make_global(8u8);
    let glob_last_genre = app.make_global(genre_param);
    let glob_loop_flash = app.make_global(0u8);

    let (
        st_slots,
        st_count,
        st_genre,
        st_scrub,
        st_tension,
        st_div,
        st_auto,
        st_meander,
        st_running,
        st_clip_active,
        st_clip_len,
        st_clip_bars,
        st_clip_deg,
        st_clip_on,
        st_clip_dur,
    ) = storage.query(|s| {
        (
            s.slots,
            s.slot_count,
            s.genre as usize,
            s.scrub,
            s.tension,
            s.div_fader,
            s.mode_auto,
            s.meander,
            s.auto_running,
            s.clip_active,
            s.clip_len,
            s.clip_bars,
            s.clip_deg,
            s.clip_on,
            s.clip_dur,
        )
    });

    let mut slots = st_slots;
    let mut slot_count = st_count;
    if slot_count == 0 {
        slot_count = load_genre_into(&mut slots, genre_param);
    }
    let genre = if st_genre < NUM_GENRES {
        st_genre
    } else {
        genre_param
    };
    // Live mode lives in storage (Shift+Button). Start mode applied below on pristine storage.
    let mut mode_auto = st_auto;
    if !st_auto
        && !st_running
        && st_scrub == 0
        && st_tension == 2048
        && st_div == 1536
        && start_mode == 1
    {
        mode_auto = true;
    }
    let meander = st_meander || auto_style == 1;

    slots_cell.set(slots);
    glob_slot_count.set(slot_count);
    glob_genre.set(if st_clip_active && st_clip_len > 0 {
        CAPTURE_SLOT
    } else {
        genre
    });
    glob_last_genre.set(genre.min(NUM_GENRES - 1));
    glob_scrub.set(st_scrub);
    glob_tension.set(st_tension);
    glob_div.set(value_to_resolution(st_div, CLOCK_RESOLUTIONS).max(1));
    glob_mode_auto.set(mode_auto);
    glob_meander.set(meander);
    // Remember play/pause across Perform↔Auto; only gated by mode_auto when ticking.
    glob_auto_running.set(st_running);
    clip_deg_cell.set(st_clip_deg);
    clip_on_cell.set(st_clip_on);
    clip_dur_cell.set(st_clip_dur);
    glob_clip_len.set(st_clip_len);
    glob_clip_bars.set(st_clip_bars.max(1));
    glob_clip_active.set(st_clip_active && st_clip_len > 0);
    {
        let count = slot_count.max(1) as usize;
        let idx = value_to_index(st_scrub, count);
        glob_slot_idx.set(idx as u8);
        glob_degree.set(slots[idx]);
    }

    leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);

    let push_ring_on = |degree: u8, on: u32| {
        // Close any dangling open event at this tick.
        let open = ring_open.get() as usize;
        if open < RING_CAP {
            let mut offs = ring_off.get();
            if offs[open] == 0 {
                offs[open] = on.max(1);
                ring_off.set(offs);
            }
        }
        let mut deg = ring_deg.get();
        let mut ons = ring_on.get();
        let mut offs = ring_off.get();
        let w = ring_write.get() as usize;
        deg[w] = degree;
        ons[w] = on;
        offs[w] = 0; // open
        ring_deg.set(deg);
        ring_on.set(ons);
        ring_off.set(offs);
        ring_open.set(w as u8);
        ring_write.set(((w + 1) % RING_CAP) as u8);
        let len = ring_len.get();
        if (len as usize) < RING_CAP {
            ring_len.set(len + 1);
        }
    };

    let close_ring = |off: u32| {
        let open = ring_open.get() as usize;
        if open >= RING_CAP {
            return;
        }
        let mut offs = ring_off.get();
        let ons = ring_on.get();
        let t = off.max(ons[open].wrapping_add(1));
        offs[open] = t;
        ring_off.set(offs);
        ring_open.set(255);
    };

    let clock_watch = async {
        let mut auto_step: usize = 0;
        let mut current_degree: u8 = glob_degree.get();
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick => {
                    if !(glob_mode_auto.get() && glob_auto_running.get()) {
                        continue;
                    }
                    let clkn = ticks() as u32;

                    // Timed capture clip playback
                    if glob_clip_active.get() && glob_clip_len.get() > 0 {
                        if glob_clip_armed.get() {
                            if clkn.is_multiple_of(TICKS_PER_BAR) {
                                glob_clip_origin.set(clkn);
                                glob_clip_armed.set(false);
                                glob_loop_flash.set(40);
                            }
                            continue;
                        }
                        let bars = glob_clip_bars.get().max(1) as u32;
                        let loop_len = bars.saturating_mul(TICKS_PER_BAR).max(1);
                        let phase = clkn.wrapping_sub(glob_clip_origin.get()) % loop_len;
                        if phase == 0 {
                            glob_loop_flash.set(40);
                        }
                        let n = glob_clip_len.get() as usize;
                        let degs = clip_deg_cell.get();
                        let ons = clip_on_cell.get();
                        let durs = clip_dur_cell.get();
                        for i in 0..n {
                            if ons[i] as u32 == phase {
                                current_degree = degs[i];
                                glob_degree.set(degs[i]);
                                glob_slot_idx.set(i as u8);
                                pending_degree.set(degs[i]);
                                pending_record.set(false);
                                pending_play.set(true);
                            }
                            let end = (ons[i] as u32).saturating_add(durs[i] as u32);
                            let off_phase = if end >= loop_len {
                                loop_len.saturating_sub(1)
                            } else {
                                end
                            };
                            if off_phase == phase && durs[i] > 0 {
                                pending_release.set(true);
                            }
                        }
                        continue;
                    }

                    // Genre progression / meander
                    let div = glob_div.get().max(1);
                    if clkn.is_multiple_of(div) {
                        let slots_now = slots_cell.get();
                        let count = glob_slot_count.get().max(1) as usize;
                        let degree = if glob_meander.get() {
                            let tension = if cv_dest == DEST_MACRO {
                                mod_u16(glob_tension.get(), glob_cv_val.get())
                            } else {
                                glob_tension.get()
                            };
                            if tension > 512 {
                                let g = glob_last_genre.get().min(NUM_GENRES - 1);
                                let genre = GenrePreset::get(g);
                                let next = pick_markov(current_degree, genre.markov, &die);
                                if tension < 3000 && die.roll() > tension {
                                    slots_now[auto_step % count]
                                } else {
                                    next
                                }
                            } else {
                                slots_now[auto_step % count]
                            }
                        } else {
                            slots_now[auto_step % count]
                        };
                        current_degree = degree;
                        glob_slot_idx.set((auto_step % count) as u8);
                        pending_degree.set(degree);
                        pending_record.set(false);
                        pending_play.set(true);
                        auto_step = auto_step.wrapping_add(1);
                    }
                    if clkn % div == (div * 80 / 100).clamp(1, div.saturating_sub(1)) {
                        pending_release.set(true);
                    }
                }
                ClockEvent::Stop | ClockEvent::Reset => {
                    pending_release.set(true);
                    auto_step = 0;
                    if glob_clip_active.get() {
                        glob_clip_armed.set(true);
                    }
                }
                _ => {}
            }
        }
    };

    let engine = async {
        let mut sounding: Vec<u8, SOUNDING_CAP> = Vec::new();
        let mut current_degree: u8 = glob_degree.get();

        let release_all = async |sounding: &mut Vec<u8, SOUNDING_CAP>| {
            for n in sounding.iter() {
                midi.send_note_off(MidiNote::from(*n)).await;
            }
            sounding.clear();
        };

        let play_degree = async |sounding: &mut Vec<u8, SOUNDING_CAP>, degree: u8, record: bool| {
            for n in sounding.iter() {
                midi.send_note_off(MidiNote::from(*n)).await;
            }
            sounding.clear();
            let notes = build_chord(root_midi, key, degree, voicing);
            for n in notes.iter() {
                midi.send_note_on(MidiNote::from(*n), vel12).await;
                let _ = sounding.push(*n);
            }
            if record {
                push_ring_on(degree, ticks() as u32);
            }
            glob_degree.set(degree);
            glob_activity.set(255);
        };

        loop {
            if panic_flag.get() {
                release_all(&mut sounding).await;
                const ALL_SOUND_OFF: u8 = 120;
                const ALL_NOTES_OFF: u8 = 123;
                midi.send_cc(MidiCc::from(ALL_SOUND_OFF), 0).await;
                midi.send_cc(MidiCc::from(ALL_NOTES_OFF), 0).await;
                sounding.clear();
                chord_held.set(false);
                pending_play.set(false);
                pending_release.set(false);
                panic_flag.set(false);
                glob_activity.set(0);
            }

            if pending_release.get() {
                pending_release.set(false);
                release_all(&mut sounding).await;
            }

            if pending_play.get() {
                pending_play.set(false);
                let degree = pending_degree.get();
                let record = pending_record.get();
                pending_record.set(false);
                current_degree = degree;
                play_degree(&mut sounding, degree, record).await;
            }

            // Perform: chord hold (Auto voice comes only from pending_play).
            if !glob_mode_auto.get() {
                if chord_held.get() {
                    let deg = glob_degree.get();
                    if sounding.is_empty() || current_degree != deg {
                        current_degree = deg;
                        play_degree(&mut sounding, deg, true).await;
                    }
                } else if !sounding.is_empty() {
                    close_ring(ticks() as u32);
                    release_all(&mut sounding).await;
                }
            }

            if glob_activity.get() > 0 {
                glob_activity.set(glob_activity.get().saturating_sub(8));
            }
            if glob_capture_flash.get() > 0 {
                glob_capture_flash.set(glob_capture_flash.get().saturating_sub(4));
            }
            if glob_loop_flash.get() > 0 {
                glob_loop_flash.set(glob_loop_flash.get().saturating_sub(2));
            }

            app.delay_millis(1).await;
        }
    };

    let button_handler = async {
        loop {
            let (_chan, shift) = buttons.wait_for_any_down().await;
            if shift {
                // Shift+Button: Perform ↔ Auto — keep play/pause memory either way.
                let next = !glob_mode_auto.get();
                glob_mode_auto.set(next);
                if !next {
                    chord_held.set(false);
                    pending_release.set(true);
                }
                storage.modify_and_save(|s| {
                    s.mode_auto = next;
                    s.auto_running = glob_auto_running.get();
                });
                // consume until up so we don't also treat as short press
                let _ = buttons.wait_for_up(0).await;
                continue;
            }

            long_press_fired.set(false);
            if glob_mode_auto.get() {
                // Auto: short = play/pause chord autoplay (global clock is Scene+Shift only)
                buttons.wait_for_up(0).await;
                if long_press_fired.get() {
                    // Capture already handled in long_press
                    continue;
                }
                let playing = !glob_auto_running.get();
                glob_auto_running.set(playing);
                if !playing {
                    // Pause: silence current chord; clock keeps running independently.
                    pending_release.set(true);
                }
                storage.modify_and_save(|s| s.auto_running = playing);
            } else {
                // Perform: hold = chord; release = note off (unless long-press capture)
                let count = glob_slot_count.get().max(1) as usize;
                let idx = value_to_index(glob_scrub.get(), count);
                let slots_now = slots_cell.get();
                let degree = slots_now[idx];
                glob_slot_idx.set(idx as u8);
                glob_degree.set(degree);
                chord_held.set(true);

                buttons.wait_for_up(0).await;
                chord_held.set(false);
                // long-press capture does not need extra handling here
            }
        }
    };

    let long_press = async {
        loop {
            let (_chan, shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            if shift {
                // Shift+Long: Panic
                panic_flag.set(true);
                continue;
            }

            if glob_mode_auto.get() {
                // Auto Long-Press: clear capture clip, reload genre preset.
                glob_clip_active.set(false);
                glob_clip_armed.set(false);
                pending_release.set(true);
                let g = glob_last_genre.get().min(NUM_GENRES - 1);
                glob_genre.set(g);
                let mut slots = slots_cell.get();
                let n = load_genre_into(&mut slots, g);
                slots_cell.set(slots);
                glob_slot_count.set(n);
                storage.modify_and_save(|s| {
                    s.clip_active = false;
                    s.clip_len = 0;
                    s.genre = g as u8;
                    s.slots = slots;
                    s.slot_count = n;
                });
                glob_capture_flash.set(96);
                continue;
            }

            // Perform Long-Press: capture last N bars as timed clip → Auto + Play, arm downbeat.
            close_ring(ticks() as u32);
            chord_held.set(false);
            pending_release.set(true);

            let bars = CAPTURE_BARS[capture_len];
            let now = ticks() as u32;
            let (n, degs, ons, durs) = build_clip_from_ring(
                bars,
                now,
                ring_len.get() as usize,
                ring_write.get() as usize,
                &ring_deg.get(),
                &ring_on.get(),
                &ring_off.get(),
                ring_open.get(),
            );

            if n == 0 {
                glob_capture_flash.set(64);
                continue;
            }

            clip_deg_cell.set(degs);
            clip_on_cell.set(ons);
            clip_dur_cell.set(durs);
            glob_clip_len.set(n);
            glob_clip_bars.set(bars.min(255) as u8);
            glob_clip_active.set(true);
            glob_clip_armed.set(true);
            glob_genre.set(CAPTURE_SLOT);
            glob_mode_auto.set(true);
            glob_auto_running.set(true);
            glob_capture_flash.set(255);

            storage.modify_and_save(|s| {
                s.clip_active = true;
                s.clip_len = n;
                s.clip_bars = bars.min(255) as u8;
                s.clip_deg = degs;
                s.clip_on = ons;
                s.clip_dur = durs;
                s.mode_auto = true;
                s.auto_running = true;
            });
        }
    };

    let fader_handler = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;
            let layer = glob_latch.get();
            let target = match layer {
                LatchLayer::Main => {
                    if glob_mode_auto.get() {
                        storage.query(|s| s.tension)
                    } else {
                        storage.query(|s| s.scrub)
                    }
                }
                LatchLayer::Alt => {
                    // Genres + Capture slot (when a clip exists)
                    let picks = if glob_clip_active.get() && glob_clip_len.get() > 0 {
                        NUM_GENRES + 1
                    } else {
                        NUM_GENRES
                    };
                    let g = storage.query(|s| {
                        if s.clip_active && s.clip_len > 0 && glob_genre.get() == CAPTURE_SLOT {
                            CAPTURE_SLOT
                        } else {
                            s.genre as usize
                        }
                    });
                    ((g * 4095) / picks.max(1)) as u16
                }
                LatchLayer::Third => storage.query(|s| s.div_fader),
            };
            if let Some(new_value) = latch.update(fader.get_value(), layer, target) {
                match layer {
                    LatchLayer::Main => {
                        if glob_mode_auto.get() {
                            glob_tension.set(new_value);
                            storage.modify_and_save(|s| s.tension = new_value);
                        } else {
                            glob_scrub.set(new_value);
                            let count = glob_slot_count.get().max(1) as usize;
                            let idx = value_to_index(new_value, count);
                            glob_slot_idx.set(idx as u8);
                            let slots_now = slots_cell.get();
                            glob_degree.set(slots_now[idx]);
                            storage.modify_and_save(|s| s.scrub = new_value);
                        }
                    }
                    LatchLayer::Alt => {
                        let picks = if glob_clip_active.get() && glob_clip_len.get() > 0 {
                            NUM_GENRES + 1
                        } else {
                            NUM_GENRES
                        };
                        let g = value_to_index(new_value, picks);
                        if g == CAPTURE_SLOT {
                            if glob_clip_len.get() > 0 {
                                glob_genre.set(CAPTURE_SLOT);
                                glob_clip_active.set(true);
                                glob_clip_armed.set(true);
                                pending_release.set(true);
                                storage.modify_and_save(|s| s.clip_active = true);
                            }
                        } else if g != glob_last_genre.get() || glob_clip_active.get() {
                            glob_last_genre.set(g);
                            glob_genre.set(g);
                            glob_clip_active.set(false);
                            glob_clip_armed.set(false);
                            pending_release.set(true);
                            let mut slots = slots_cell.get();
                            let n = load_genre_into(&mut slots, g);
                            slots_cell.set(slots);
                            glob_slot_count.set(n);
                            storage.modify_and_save(|s| {
                                s.genre = g as u8;
                                s.slots = slots;
                                s.slot_count = n;
                                s.clip_active = false;
                            });
                        }
                    }
                    LatchLayer::Third => {
                        glob_div.set(value_to_resolution(new_value, CLOCK_RESOLUTIONS).max(1));
                        storage.modify_and_save(|s| s.div_fader = new_value);
                    }
                }
            }
        }
    };

    let led_handler = async {
        let mut prev_gate_high = false;
        loop {
            app.delay_millis(1).await;
            let in_val = attenuate_bipolar(in_jack.get_value(), cv_att);
            glob_cv_val.set(in_val);
            if cv_dest == DEST_PANIC {
                let high = in_val >= TRIG_HIGH;
                if high && !prev_gate_high {
                    panic_flag.set(true);
                }
                prev_gate_high = high;
            } else {
                prev_gate_high = false;
                // Macro: Perform scrub / Auto tension (tension applied at clock read).
                if !glob_mode_auto.get() {
                    let scrub = mod_u16(glob_scrub.get(), in_val);
                    let count = glob_slot_count.get().max(1) as usize;
                    let idx = value_to_index(scrub, count);
                    glob_slot_idx.set(idx as u8);
                    let slots_now = slots_cell.get();
                    glob_degree.set(slots_now[idx]);
                }
            }
            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch.set(layer);

            let flash = glob_capture_flash.get();
            let loop_flash = glob_loop_flash.get();
            if flash > 0 {
                leds.set(0, Led::Button, Color::White, Brightness::Custom(flash));
            } else if loop_flash > 0 && glob_mode_auto.get() && glob_clip_active.get() {
                leds.set(0, Led::Button, Color::White, Brightness::Custom(loop_flash));
            }

            match layer {
                LatchLayer::Main => {
                    let slot = glob_slot_idx.get() as u16;
                    let count = glob_slot_count.get().max(1) as u16;
                    let meter = (slot * 4095) / count.max(1);
                    let led = split_unsigned_value(meter);
                    let pulse = glob_activity.get();
                    // Perform = app color; Auto = orange (bright=play, dim=pause).
                    let (fader_color, button_color, button_bright) = if glob_mode_auto.get() {
                        if glob_auto_running.get() {
                            (Color::Orange, Color::Orange, Brightness::High)
                        } else {
                            (Color::Orange, Color::Orange, Brightness::Low)
                        }
                    } else {
                        (led_color, led_color, LED_BRIGHTNESS)
                    };
                    leds.set(
                        0,
                        Led::Top,
                        fader_color,
                        Brightness::Custom(led[0].max(pulse)),
                    );
                    leds.set(
                        0,
                        Led::Bottom,
                        fader_color,
                        Brightness::Custom(led[1].max(pulse / 2)),
                    );
                    if flash == 0 && loop_flash == 0 {
                        leds.set(0, Led::Button, button_color, button_bright);
                    }
                }
                LatchLayer::Alt => {
                    let picks = if glob_clip_active.get() && glob_clip_len.get() > 0 {
                        NUM_GENRES + 1
                    } else {
                        NUM_GENRES
                    };
                    let g = glob_genre.get().min(picks - 1);
                    let color = if g == CAPTURE_SLOT {
                        CAPTURE_COLOR
                    } else {
                        GENRE_COLORS[g.min(NUM_GENRES - 1)]
                    };
                    let meter = (g as u16 * 4095) / (picks as u16).max(1);
                    let led = split_unsigned_value(meter);
                    leds.set(0, Led::Top, color, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, color, Brightness::Custom(led[1]));
                    if flash == 0 {
                        leds.set(0, Led::Button, color, Brightness::High);
                    }
                }
                LatchLayer::Third => {
                    let div_f = storage.query(|s| s.div_fader);
                    let led = split_unsigned_value(div_f);
                    leds.set(0, Led::Top, Color::Cyan, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Cyan, Brightness::Custom(led[1]));
                    if flash == 0 {
                        leds.set(0, Led::Button, Color::Cyan, LED_BRIGHTNESS);
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
                        slots,
                        count,
                        genre,
                        scrub,
                        tension,
                        div_f,
                        auto,
                        meander,
                        running,
                        clip_active,
                        clip_len,
                        clip_bars,
                        clip_deg,
                        clip_on,
                        clip_dur,
                    ) = storage.query(|s| {
                        (
                            s.slots,
                            s.slot_count,
                            s.genre as usize,
                            s.scrub,
                            s.tension,
                            s.div_fader,
                            s.mode_auto,
                            s.meander,
                            s.auto_running,
                            s.clip_active,
                            s.clip_len,
                            s.clip_bars,
                            s.clip_deg,
                            s.clip_on,
                            s.clip_dur,
                        )
                    });
                    slots_cell.set(slots);
                    glob_slot_count.set(count.max(1));
                    let g = genre.min(NUM_GENRES - 1);
                    glob_last_genre.set(g);
                    glob_scrub.set(scrub);
                    glob_tension.set(tension);
                    glob_div.set(value_to_resolution(div_f, CLOCK_RESOLUTIONS).max(1));
                    glob_mode_auto.set(auto);
                    glob_meander.set(meander);
                    glob_auto_running.set(running);
                    clip_deg_cell.set(clip_deg);
                    clip_on_cell.set(clip_on);
                    clip_dur_cell.set(clip_dur);
                    glob_clip_len.set(clip_len);
                    glob_clip_bars.set(clip_bars.max(1));
                    let use_clip = clip_active && clip_len > 0;
                    glob_clip_active.set(use_clip);
                    glob_clip_armed.set(use_clip);
                    glob_genre.set(if use_clip { CAPTURE_SLOT } else { g });
                    panic_flag.set(true);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join(
        long_press,
        join(
            clock_watch,
            join5(
                engine,
                button_handler,
                fader_handler,
                led_handler,
                scene_handler,
            ),
        ),
    )
    .await;
}
