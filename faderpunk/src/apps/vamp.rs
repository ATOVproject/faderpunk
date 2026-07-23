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
    utils::{split_unsigned_value, value_to_index, value_to_resolution},
    AppIcon, Brightness, ClockDivision, Color, Config, Key, MidiCc, MidiChannel, MidiNote, MidiOut,
    Param, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Die, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 11;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const VAMP_CAP: usize = 32;
const RING_CAP: usize = 64;
const SOUNDING_CAP: usize = 8;
const TICKS_PER_BAR: u32 = 96;
const NUM_GENRES: usize = 8;
const NUM_DEGREES: usize = 7;

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

const CLOCK_RESOLUTIONS: &[u16] = &[384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

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
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
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
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Active vamp degrees (I=0 … vii=6).
    slots: [u8; VAMP_CAP],
    slot_count: u8,
    genre: u8,
    scrub: u16,
    tension: u16,
    div_fader: u16,
    mode_auto: bool,
    meander: bool,
    auto_running: bool,
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
            div_fader: 2048,
            mode_auto: false,
            meander: false,
            auto_running: false,
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

#[embassy_executor::task(pool_size = 4)]
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
    let chord_held = app.make_global(false);
    let ring_write = app.make_global(0u8);
    let ring_len = app.make_global(0u8);
    // Clock watch → voice engine (never await MIDI inside the clock subscriber).
    let pending_play = app.make_global(false);
    let pending_degree = app.make_global(0u8);
    let pending_release = app.make_global(false);
    let pending_record = app.make_global(false);

    // Shared vamp slots + ring (mutated under single-writer futures via globals for indices).
    let slots_cell = app.make_global([0u8; VAMP_CAP]);
    let ring_deg = app.make_global([0u8; RING_CAP]);
    let ring_tick = app.make_global([0u32; RING_CAP]);

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
        && st_div == 2048
        && start_mode == 1
    {
        mode_auto = true;
    }
    let meander = st_meander || auto_style == 1;

    slots_cell.set(slots);
    glob_slot_count.set(slot_count);
    glob_genre.set(genre);
    glob_scrub.set(st_scrub);
    glob_tension.set(st_tension);
    glob_div.set(value_to_resolution(st_div, CLOCK_RESOLUTIONS).max(1));
    glob_mode_auto.set(mode_auto);
    glob_meander.set(meander);
    glob_auto_running.set(st_running && mode_auto);
    {
        let count = slot_count.max(1) as usize;
        let idx = value_to_index(st_scrub, count);
        glob_slot_idx.set(idx as u8);
        glob_degree.set(slots[idx]);
    }

    leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);

    let push_ring = |degree: u8, tick: u32| {
        let mut deg = ring_deg.get();
        let mut tks = ring_tick.get();
        let w = ring_write.get() as usize;
        deg[w] = degree;
        tks[w] = tick;
        ring_deg.set(deg);
        ring_tick.set(tks);
        ring_write.set(((w + 1) % RING_CAP) as u8);
        let len = ring_len.get();
        if (len as usize) < RING_CAP {
            ring_len.set(len + 1);
        }
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
                    let div = glob_div.get().max(1);
                    let clkn = ticks() as u32;
                    if clkn.is_multiple_of(div) {
                        let slots_now = slots_cell.get();
                        let count = glob_slot_count.get().max(1) as usize;
                        let degree = if glob_meander.get() {
                            let tension = glob_tension.get();
                            if tension > 512 {
                                let genre = GenrePreset::get(glob_genre.get());
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
                        pending_record.set(true);
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
                push_ring(degree, ticks() as u32);
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
                    release_all(&mut sounding).await;
                }
            }

            if glob_activity.get() > 0 {
                glob_activity.set(glob_activity.get().saturating_sub(8));
            }
            if glob_capture_flash.get() > 0 {
                glob_capture_flash.set(glob_capture_flash.get().saturating_sub(4));
            }

            app.delay_millis(1).await;
        }
    };

    let button_handler = async {
        loop {
            let (_chan, shift) = buttons.wait_for_any_down().await;
            if shift {
                // Shift+Button: Perform ↔ Auto
                let next = !glob_mode_auto.get();
                glob_mode_auto.set(next);
                if !next {
                    glob_auto_running.set(false);
                    chord_held.set(false);
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
            // Long-Press: Capture last N bars into vamp buffer
            let bars = CAPTURE_BARS[capture_len];
            let window = bars.saturating_mul(TICKS_PER_BAR);
            let now = ticks() as u32;
            let len = ring_len.get() as usize;
            let write = ring_write.get() as usize;
            let deg = ring_deg.get();
            let tks = ring_tick.get();

            let mut captured: Vec<u8, VAMP_CAP> = Vec::new();
            if len > 0 {
                for i in 0..len {
                    let idx = if len < RING_CAP {
                        i
                    } else {
                        (write + i) % RING_CAP
                    };
                    let age = now.wrapping_sub(tks[idx]);
                    if age < window && age < (u32::MAX / 2) && captured.len() < VAMP_CAP {
                        let _ = captured.push(deg[idx]);
                    }
                }
            }

            if !captured.is_empty() {
                let mut slots = [0u8; VAMP_CAP];
                let n = captured.len();
                slots[..n].copy_from_slice(&captured);
                slots_cell.set(slots);
                glob_slot_count.set(n as u8);
                storage.modify_and_save(|s| {
                    s.slots = slots;
                    s.slot_count = n as u8;
                });
                glob_capture_flash.set(255);
                chord_held.set(false);
            } else {
                // Empty capture: brief dim flash
                glob_capture_flash.set(64);
            }
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
                    // Map genre to fader center of zone for latch target
                    let g = storage.query(|s| s.genre) as usize;
                    ((g * 4095) / NUM_GENRES.max(1)) as u16
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
                        let g = value_to_index(new_value, NUM_GENRES);
                        if g != glob_genre.get() {
                            glob_genre.set(g);
                            let mut slots = slots_cell.get();
                            let n = load_genre_into(&mut slots, g);
                            slots_cell.set(slots);
                            glob_slot_count.set(n);
                            storage.modify_and_save(|s| {
                                s.genre = g as u8;
                                s.slots = slots;
                                s.slot_count = n;
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
        loop {
            app.delay_millis(1).await;
            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch.set(layer);

            let flash = glob_capture_flash.get();
            if flash > 0 {
                leds.set(0, Led::Button, Color::White, Brightness::Custom(flash));
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
                    if flash == 0 {
                        leds.set(0, Led::Button, button_color, button_bright);
                    }
                }
                LatchLayer::Alt => {
                    let g = glob_genre.get().min(NUM_GENRES - 1);
                    let color = GENRE_COLORS[g];
                    let meter = (g as u16 * 4095) / (NUM_GENRES as u16).max(1);
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
                    let (slots, count, genre, scrub, tension, div_f, auto, meander, running) =
                        storage.query(|s| {
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
                            )
                        });
                    slots_cell.set(slots);
                    glob_slot_count.set(count.max(1));
                    glob_genre.set(genre.min(NUM_GENRES - 1));
                    glob_scrub.set(scrub);
                    glob_tension.set(tension);
                    glob_div.set(value_to_resolution(div_f, CLOCK_RESOLUTIONS).max(1));
                    glob_mode_auto.set(auto);
                    glob_meander.set(meander);
                    glob_auto_running.set(running && auto);
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
