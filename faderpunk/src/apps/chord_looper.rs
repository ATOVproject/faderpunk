//! Chord Looper — a silent chord progression runs in the background, the
//! button plays "the chord that is playing right now" and records the rhythm
//! of the presses into a looping 4/8 bar sequence.
//!
//! * Fader: pick one of 8 chord progressions (bottom LED colour = selection).
//! * Button: play the current chord (hold = sustain). Presses are recorded and
//!   looped; the loop plays whichever chord the progression has at that point,
//!   so changing the progression re-harmonises the recorded rhythm.
//! * Shift + Button: erase the recorded sequence.
//! * Jack: gate input that plays the current chord like the button. Whether
//!   gate hits are recorded is a parameter.
//!
//! Chord tones are authored relative to the tonic and run through the global
//! quantizer, so they snap to the active scale (use a scale that matches the
//! progression, or turn the quantizer off / bypass it for exact chords).

use core::cell::Cell;

use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use midly::num::u7;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config, Key,
    MidiChannel, MidiNote, MidiOut, Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, Arr, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 7;

/// 24 PPQN, 4/4
const TICKS_PER_BAR: u64 = 96;
/// Recording grid: 1/16 note
const STEP_TICKS: u64 = 6;
const STEPS_PER_BAR: usize = 16;
const MAX_STEPS: usize = 128;
/// Gate input threshold (0-10V range, ~1V)
const GATE_THRESHOLD: u16 = 406;
const LED_ON: Brightness = Brightness::High;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Chord Looper",
    "Background chord progression, play and loop chords with the button",
    Color::Pink,
    AppIcon::Note,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "Base note" })
.add_param(Param::Enum {
    name: "Length",
    variants: &["4 bars", "8 bars"],
})
.add_param(Param::bool {
    name: "Record gate input",
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
.add_param(Param::MidiOut)
.add_param(Param::bool {
    name: "Bypass quantizer",
});

pub struct Params {
    midi_channel: MidiChannel,
    base_note: MidiNote,
    length: usize,
    record_gate: bool,
    color: Color,
    midi_out: MidiOut,
    bypass: bool,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            base_note: MidiNote::from_value(values[1]),
            length: usize::from_value(values[2]),
            record_gate: bool::from_value(values[3]),
            color: Color::from_value(values[4]),
            midi_out: MidiOut::from_value(values[5]),
            bypass: bool::from_value(values[6]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.base_note.into()).unwrap();
        vec.push(self.length.into()).unwrap();
        vec.push(self.record_gate.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.bypass.into()).unwrap();
        vec
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Storage {
    /// Raw fader value that selected the progression (for latching)
    fader_saved: u16,
    /// Recorded hits: `lens[step]` = hold length in steps, 0 = no hit
    lens: Arr<u8, MAX_STEPS>,
}

impl AppStorage for Storage {}

// ---------------------------------------------------------------------------
// Chord progressions (standard, public-domain harmonic patterns)
// ---------------------------------------------------------------------------

type Quality = &'static [u8];

const MAJ: Quality = &[0, 4, 7];
const MIN: Quality = &[0, 3, 7];
const DOM7: Quality = &[0, 4, 7, 10];
const MAJ7: Quality = &[0, 4, 7, 11];
const MIN7: Quality = &[0, 3, 7, 10];

/// Root as semitones above the tonic, plus chord quality.
#[derive(Clone, Copy)]
struct Chord(u8, Quality);

/// One chord per bar. In 4-bar mode only the first four bars play, so each
/// progression's first half is a complete turn-around.
const PROGRESSIONS: [[Chord; 8]; 8] = [
    // 0: Pop — I V vi IV | I V IV V
    [
        Chord(0, MAJ),
        Chord(7, MAJ),
        Chord(9, MIN),
        Chord(5, MAJ),
        Chord(0, MAJ),
        Chord(7, MAJ),
        Chord(5, MAJ),
        Chord(7, MAJ),
    ],
    // 1: 50s doo-wop — I vi IV V | I vi ii V
    [
        Chord(0, MAJ),
        Chord(9, MIN),
        Chord(5, MAJ),
        Chord(7, MAJ),
        Chord(0, MAJ),
        Chord(9, MIN),
        Chord(2, MIN),
        Chord(7, MAJ),
    ],
    // 2: Jazz ii-V-I — ii7 V7 Imaj7 Imaj7 | ii7 V7 Imaj7 vi7
    [
        Chord(2, MIN7),
        Chord(7, DOM7),
        Chord(0, MAJ7),
        Chord(0, MAJ7),
        Chord(2, MIN7),
        Chord(7, DOM7),
        Chord(0, MAJ7),
        Chord(9, MIN7),
    ],
    // 3: Rhythm changes turn-around — Imaj7 vi7 ii7 V7 | iii7 VI7 ii7 V7
    [
        Chord(0, MAJ7),
        Chord(9, MIN7),
        Chord(2, MIN7),
        Chord(7, DOM7),
        Chord(4, MIN7),
        Chord(9, DOM7),
        Chord(2, MIN7),
        Chord(7, DOM7),
    ],
    // 4: Minor epic — i VI III VII | i VI iv V
    [
        Chord(0, MIN),
        Chord(8, MAJ),
        Chord(3, MAJ),
        Chord(10, MAJ),
        Chord(0, MIN),
        Chord(8, MAJ),
        Chord(5, MIN),
        Chord(7, MAJ),
    ],
    // 5: Andalusian cadence — i VII VI V (x2)
    [
        Chord(0, MIN),
        Chord(10, MAJ),
        Chord(8, MAJ),
        Chord(7, MAJ),
        Chord(0, MIN),
        Chord(10, MAJ),
        Chord(8, MAJ),
        Chord(7, MAJ),
    ],
    // 6: Canon (Pachelbel) — I V vi iii IV I IV V
    [
        Chord(0, MAJ),
        Chord(7, MAJ),
        Chord(9, MIN),
        Chord(4, MIN),
        Chord(5, MAJ),
        Chord(0, MAJ),
        Chord(5, MAJ),
        Chord(7, MAJ),
    ],
    // 7: Eight-bar blues — I7 V7 IV7 IV7 | I7 V7 I7 V7
    [
        Chord(0, DOM7),
        Chord(7, DOM7),
        Chord(5, DOM7),
        Chord(5, DOM7),
        Chord(0, DOM7),
        Chord(7, DOM7),
        Chord(0, DOM7),
        Chord(7, DOM7),
    ],
];

/// Bottom LED colour per progression
const PROGRESSION_COLORS: [Color; 8] = [
    Color::Blue,
    Color::Green,
    Color::Rose,
    Color::Orange,
    Color::Cyan,
    Color::Pink,
    Color::Violet,
    Color::Yellow,
];

fn progression_from_fader(value: u16) -> usize {
    (value as usize * PROGRESSIONS.len() / 4096).min(PROGRESSIONS.len() - 1)
}

/// Who is currently holding the live chord
const OWNER_NONE: u8 = 0;
const OWNER_BUTTON: u8 = 1;
const OWNER_GATE: u8 = 2;

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            base_note: MidiNote::from(48),
            length: 1,
            record_gate: false,
            color: Color::Pink,
            midi_out: MidiOut::default(),
            bypass: false,
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
    let (midi_channel, base_note, length, record_gate, led_color, midi_out, bypass) =
        params.query(|p| {
            (
                p.midi_channel,
                p.base_note,
                p.length,
                p.record_gate,
                p.color,
                p.midi_out,
                p.bypass,
            )
        });

    let bars: usize = if length == 0 { 4 } else { 8 };
    let total_steps = bars * STEPS_PER_BAR;
    let total_ticks = bars as u64 * TICKS_PER_BAR;

    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let midi = app.use_midi_output(midi_out, midi_channel, false);
    let quantizer = app.use_quantizer(Range::_0_10V, VoltPerOct::Standard, bypass);
    let gate_in = app.make_in_jack(0, Range::_0_10V).await;

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    // Currently selected progression
    let prog = Cell::new(progression_from_fader(storage.query(|s| s.fader_saved)));
    // Chord notes that are currently sounding (note numbers, count)
    let sounding: Cell<([u8; 4], usize)> = Cell::new(([0; 4], 0));
    // Whether the sounding chord was started by loop playback (vs. live play)
    let playback_sounding = Cell::new(false);
    // Step at which loop playback should release its chord
    let release_at: Cell<Option<usize>> = Cell::new(None);
    // Live chord holder, and the recording in progress (step, start tick)
    let owner = Cell::new(OWNER_NONE);
    let recording: Cell<Option<(usize, u64)>> = Cell::new(None);

    let has_sequence = || storage.query(|s| s.lens.get()[..total_steps].iter().any(|&l| l > 0));

    let idle_button = || {
        let brightness = if has_sequence() {
            Brightness::Mid
        } else {
            Brightness::Low
        };
        leds.set(0, Led::Button, led_color, brightness);
    };
    let show_progression = || {
        leds.set(
            0,
            Led::Bottom,
            PROGRESSION_COLORS[prog.get()],
            Brightness::Mid,
        );
    };

    let stop_chord = async || {
        let (notes, count) = sounding.replace(([0; 4], 0));
        for note in notes.iter().take(count) {
            midi.send_note_off(MidiNote::from(*note)).await;
        }
        playback_sounding.set(false);
    };

    // Chord of the bar containing `tick` (bar 0 when the clock hasn't run yet)
    let chord_at = |tick: Option<u64>| {
        let bar = tick.map_or(0, |t| ((t % total_ticks) / TICKS_PER_BAR) as usize);
        PROGRESSIONS[prog.get()][bar]
    };

    let start_chord = async |chord: Chord| {
        stop_chord().await;

        // Authored semitones relative to the tonic -> quantizer input counts
        // (0-10V, 1V/oct: 120 semitones over 4095 counts) -> scale-snapped note
        let (key, tonic) = quantizer.get_scale().await;
        let tonic = if key == Key::Off { 0 } else { tonic as i32 };

        let mut notes = [0u8; 4];
        let mut count = 0;
        for interval in chord.1.iter() {
            let semitones = tonic + chord.0 as i32 + *interval as i32;
            let counts = ((semitones * 4095 + 60) / 120) as u16;
            let pitch = quantizer.get_quantized_note(counts).await;
            // `as_midi` is octave-0 = 12; the base note is the chord's floor
            let midi_note = u7::from(pitch.as_midi()).as_int() as i32 - 12
                + u7::from(base_note).as_int() as i32;
            let midi_note = midi_note.clamp(0, 127) as u8;
            if count < notes.len() && !notes[..count].contains(&midi_note) {
                notes[count] = midi_note;
                count += 1;
            }
        }
        for note in notes.iter().take(count) {
            midi.send_note_on(MidiNote::from(*note), 4095).await;
        }
        sounding.set((notes, count));
        leds.set(0, Led::Button, led_color, LED_ON);
    };

    let press = async |who: u8, record: bool| {
        let tick = app.current_tick();
        start_chord(chord_at(tick)).await;
        playback_sounding.set(false);
        release_at.set(None);
        owner.set(who);

        recording.set(match tick {
            Some(t) if record => {
                let pos = t % total_ticks;
                let mut step = ((pos + STEP_TICKS / 2) / STEP_TICKS) as usize;
                // Don't let rounding push the hit into the next bar's chord
                if step.is_multiple_of(STEPS_PER_BAR) && step != (pos / STEP_TICKS) as usize {
                    step = (pos / STEP_TICKS) as usize;
                }
                Some((step % total_steps, t))
            }
            _ => None,
        });
    };

    let release = async |who: u8| {
        if owner.get() != who {
            return;
        }
        owner.set(OWNER_NONE);
        stop_chord().await;
        idle_button();

        if let Some((step, start)) = recording.take() {
            let now = app.current_tick().unwrap_or(start);
            let held = now.saturating_sub(start);
            let len = ((held + STEP_TICKS / 2) / STEP_TICKS).clamp(1, total_steps as u64);
            storage.modify_and_save(|s| s.lens.set_at(step, len as u8));
            idle_button();
        }
    };

    idle_button();
    show_progression();

    let clock_loop = async {
        loop {
            match clock.wait_for_event(ClockDivision::_6).await {
                ClockEvent::Tick(tick) => {
                    let pos = tick % total_ticks;
                    let step = (pos / STEP_TICKS) as usize;

                    // Beat indicator
                    if tick % 24 == 0 {
                        let level = if pos == 0 {
                            Brightness::High
                        } else {
                            Brightness::Mid
                        };
                        leds.set(0, Led::Top, led_color, level);
                    } else if tick % 24 == 12 {
                        leds.unset(0, Led::Top);
                    }

                    // Release a loop-played chord when its length is up
                    if release_at.get() == Some(step) {
                        release_at.set(None);
                        if playback_sounding.get() {
                            stop_chord().await;
                            idle_button();
                        }
                    }

                    // Loop playback (suppressed while a live chord is held)
                    let len = storage.query(|s| s.lens.at(step)) as usize;
                    if len > 0 && owner.get() == OWNER_NONE {
                        start_chord(chord_at(Some(tick))).await;
                        playback_sounding.set(true);
                        release_at.set(Some((step + len) % total_steps));
                    }
                }
                ClockEvent::Stop | ClockEvent::Reset => {
                    release_at.set(None);
                    if owner.get() == OWNER_NONE {
                        stop_chord().await;
                        idle_button();
                    }
                    leds.unset(0, Led::Top);
                }
                ClockEvent::Start => {}
            }
        }
    };

    let button_loop = async {
        loop {
            let shift = buttons.wait_for_down(0).await;
            if shift {
                // Shift + button: erase the recorded sequence
                recording.set(None);
                release_at.set(None);
                storage.modify_and_save(|s| s.lens.set([0; MAX_STEPS]));
                if owner.get() == OWNER_NONE {
                    stop_chord().await;
                }
                idle_button();
                buttons.wait_for_up(0).await;
                continue;
            }
            press(OWNER_BUTTON, true).await;
            buttons.wait_for_up(0).await;
            release(OWNER_BUTTON).await;
        }
    };

    let gate_loop = async {
        let mut old = 0;
        loop {
            app.delay_millis(1).await;
            let value = gate_in.get_value();
            if value >= GATE_THRESHOLD && old < GATE_THRESHOLD {
                press(OWNER_GATE, record_gate).await;
            } else if value < GATE_THRESHOLD && old >= GATE_THRESHOLD {
                release(OWNER_GATE).await;
            }
            old = value;
        }
    };

    let fader_loop = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change().await;
            let layer = glob_latch_layer.get();
            let target = match layer {
                LatchLayer::Main => storage.query(|s| s.fader_saved),
                LatchLayer::Alt | LatchLayer::Third => 0,
            };
            if let Some(value) = latch.update(faders.get_value(), layer, target) {
                if layer == LatchLayer::Main {
                    storage.modify_and_save(|s| s.fader_saved = value);
                    prog.set(progression_from_fader(value));
                    show_progression();
                }
            }
        }
    };

    let scene_and_shift = async {
        let scene_loop = async {
            loop {
                match app.wait_for_scene_event().await {
                    SceneEvent::LoadScene(scene) => {
                        storage.load_from_scene(scene).await;
                        prog.set(progression_from_fader(storage.query(|s| s.fader_saved)));
                        release_at.set(None);
                        recording.set(None);
                        if owner.get() == OWNER_NONE {
                            stop_chord().await;
                        }
                        idle_button();
                        show_progression();
                    }
                    SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
                }
            }
        };
        let shift_loop = async {
            loop {
                app.delay_millis(1).await;
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));
            }
        };
        join(scene_loop, shift_loop).await;
    };

    join5(clock_loop, button_loop, gate_loop, fader_loop, scene_and_shift).await;
}
