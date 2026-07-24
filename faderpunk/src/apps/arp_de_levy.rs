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
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Note, Param,
    Range, Value, VoltPerOct, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Die, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 10;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Reverse gesture LED feedback length (white↔off fade), same as Golden Gate / Heat Pump.
const REVERSE_FADE_MS: u16 = 500;

const POOL_CAP: usize = 16;
const MIN_PHRASE: usize = 4;
const MAX_PHRASE: usize = 16;
/// Ticks per 16th at 24 PPQN.
const STEP_DIV: u32 = 6;
/// Local Lévy table (semitones): mostly 1–3, rare 5.
const LEVY_LOCAL: [i8; 16] = [1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 5, 5];
/// Wild Lévy table: small moves still present, large jumps common.
const LEVY_WILD: [i8; 16] = [1, 2, 3, 3, 5, 5, 7, 8, 8, 10, 12, 12, 14, 17, 19, 24];

const OCT_COLORS: [Color; 4] = [Color::Blue, Color::Cyan, Color::Yellow, Color::Red];

const CV_JACK_OUT: usize = 0;
const CV_JACK_IN: usize = 1;
const DEST_MUTATION: usize = 0;
const DEST_TEXTURE: usize = 1;
const DEST_REROLL: usize = 2;
const DEST_COUNT: usize = 3;
const TRIG_HIGH: u16 = 2458;

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 100) as u32 * 4095) / 100) as u16
}

fn mod_u16(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Arp de Lévy",
    "Lévy-flight generative arpeggiator — evolve, texture, and flight character",
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
    name: "CV Dest",
    variants: &["Evolve", "Texture", "Reroll"],
})
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 100,
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
    cv_dest: usize,
    cv_att: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            note: MidiNote::from(48),
            color: Color::Rose,
            midi_out: MidiOut::default(),
            vpo: VoltPerOct::Standard,
            bypass: false,
            cv_jack: CV_JACK_OUT,
            range: Range::_0_10V,
            cv_dest: DEST_MUTATION,
            cv_att: 100,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < 6 {
            return None;
        }
        let (cv_jack, range, cv_dest, cv_att) = if values.len() >= PARAMS {
            (
                usize::from_value(values[6]).min(1),
                Range::from_value(values[7]),
                usize::from_value(values[8]).min(DEST_COUNT - 1),
                i32::from_value(values[9]).clamp(0, 100),
            )
        } else {
            (CV_JACK_OUT, Range::_0_10V, DEST_MUTATION, 100)
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
            cv_dest,
            cv_att,
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
    /// Octave span 1..=4 (cycled by Shift+long).
    octaves: u8,
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
            octaves: 2,
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

fn base_midi(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn clamp_note(n: i16) -> u8 {
    n.clamp(0, 127) as u8
}

/// α (0..=4095) blends Local→Wild: probability of sampling the wild table.
fn levy_delta(die: &Die, alpha: u16) -> i8 {
    let mag = if die.roll() < alpha {
        LEVY_WILD[(die.roll() as usize) % LEVY_WILD.len()]
    } else {
        LEVY_LOCAL[(die.roll() as usize) % LEVY_LOCAL.len()]
    };
    if die.roll() & 1 == 0 {
        mag
    } else {
        -mag
    }
}

fn mutate_pool(
    pool: &mut [u8; POOL_CAP],
    phrase_len: usize,
    lo: u8,
    hi: u8,
    alpha: u16,
    die: &Die,
) {
    if phrase_len == 0 {
        return;
    }
    let i = (die.roll() as usize) % phrase_len;
    let next = clamp_note(pool[i] as i16 + levy_delta(die, alpha) as i16);
    pool[i] = next.clamp(lo, hi);
}

fn reroll_pool(pool: &mut [u8; POOL_CAP], phrase_len: usize, lo: u8, hi: u8, die: &Die) {
    let span = (hi - lo).max(1) as u16;
    for (i, slot) in pool.iter_mut().enumerate() {
        if i < phrase_len {
            *slot = lo + ((die.roll() * span / 4095) as u8);
        } else {
            *slot = lo;
        }
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

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
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
    let (midi_out, midi_chan, base_note, led_color, vpo, bypass, cv_jack, range, cv_dest, cv_att) =
        params.query(|p| {
            (
                p.midi_out,
                p.midi_channel,
                p.note,
                p.color,
                p.vpo,
                p.bypass,
                p.cv_jack.min(1),
                p.range,
                p.cv_dest.min(DEST_COUNT - 1),
                att_from_pct(p.cv_att),
            )
        });

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
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

    let (
        fader_saved,
        shift_fader_saved,
        alpha_saved,
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
    let glob_reload_pool = app.make_global(false);

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
        leds.set(
            0,
            Led::Button,
            octave_color(glob_octaves.get()),
            LED_BRIGHTNESS,
        );
    }

    let (density0, phrase0, _) = texture_from_value(shift_fader_saved);
    let _ = density0;
    glob_phrase.set(phrase0);

    let fut_clock = async {
        let mut note_on: Option<MidiNote> = None;
        let mut pool = storage.query(|s| s.pool);
        let mut step: usize = 0;
        let mut pending_on_at: Option<u32> = None;
        let mut pending_note: Option<u8> = None;
        let mut gate_off_at: Option<u32> = None;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Stop => {
                    if let Some(n) = note_on.take() {
                        midi.send_note_off(n).await;
                    }
                    pending_on_at = None;
                    pending_note = None;
                    gate_off_at = None;
                    step = 0;
                    glob_reset.set(false);
                    if let Some(ref jack) = out_jack {
        jack.set_value(0);
    }
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                }
                ClockEvent::Tick => {
                    let clkn = ticks() as u32;
                    let octaves = clamp_octaves(glob_octaves.get());
                    let lo = base_midi(base_note);
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
                        reroll_pool(&mut pool, phrase_len, lo, hi, &die);
                        storage.modify_and_save(|s| {
                            s.pool = pool;
                            s.phrase_len = phrase_len as u8;
                        });
                    }

                    // Gate off
                    if let Some(off_at) = gate_off_at {
                        if clkn >= off_at {
                            if let Some(n) = note_on.take() {
                                midi.send_note_off(n).await;
                            }
                            leds.set(0, Led::Bottom, led_color, Brightness::Off);
                            gate_off_at = None;
                        }
                    }

                    // Delayed (swung) note-on
                    if let Some(on_at) = pending_on_at {
                        if clkn >= on_at {
                            if let Some(raw) = pending_note.take() {
                                if !glob_muted.get() {
                                    fire_note(
                                        &midi,
                                        out_jack.as_ref(),
                                        &quantizer,
                                        &leds,
                                        led_color,
                                        vpo,
                                        range,
                                        raw,
                                        &mut note_on,
                                    )
                                    .await;
                                    gate_off_at = Some(on_at + STEP_DIV / 2);
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
                                    mutate_pool(&mut pool, phrase_len, lo, hi, alpha, &die);
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
                        let raw = pool[idx].clamp(lo, hi);

                        // Density: rest if roll >= density threshold.
                        let hit = die.roll() < density && !glob_muted.get();

                        if hit {
                            if swing > 0 && (step % 2 == 1) {
                                pending_on_at = Some(clkn + swing);
                                pending_note = Some(raw);
                            } else {
                                fire_note(
                                    &midi,
                                    out_jack.as_ref(),
                                    &quantizer,
                                    &leds,
                                    led_color,
                                    vpo,
                                    range,
                                    raw,
                                    &mut note_on,
                                )
                                .await;
                                gate_off_at = Some(clkn + STEP_DIV / 2);
                            }
                        }

                        // Top LED: progress through the phrase (Main layer).
                        if glob_latch_layer.get() == LatchLayer::Main {
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom(
                                    ((step % phrase_len.max(1)) * 255 / phrase_len.max(1)) as u8,
                                ),
                            );
                        }

                        step = step.wrapping_add(1);
                        if phrase_len > 0 && step.is_multiple_of(phrase_len) {
                            step = 0;
                        }
                    }
                }
                _ => {}
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
                    // Shift + short: reverse playback direction.
                    let reversed = glob_reversed.toggle();
                    storage.modify_and_save(|s| s.reversed = reversed);
                    glob_reverse_fade_up.set(!reversed);
                    glob_reverse_fade.set(REVERSE_FADE_MS);
                }
            } else {
                long_press_fired.set(false);
                glob_fader_moved.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    // Short press: reroll the note pool.
                    glob_reroll.set(true);
                    glob_reset.set(true);
                } else if !glob_fader_moved.get() {
                    // Long press without fader move: mute.
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| s.muted = muted);
                    if muted {
                        midi.send_note_off(base_note).await;
                        leds.unset(0, Led::Button);
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(
                            0,
                            Led::Button,
                            octave_color(glob_octaves.get()),
                            LED_BRIGHTNESS,
                        );
                    }
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
                if !glob_muted.get() {
                    leds.set(
                        0,
                        Led::Button,
                        octave_color(octaves),
                        LED_BRIGHTNESS,
                    );
                }
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
                    let (fader_saved, shift_fader_saved, alpha_saved, octaves, muted, reversed) =
                        storage.query(|s| {
                            (
                                s.fader_saved,
                                s.shift_fader_saved,
                                s.alpha_saved,
                                s.octaves,
                                s.muted,
                                s.reversed,
                            )
                        });
                    glob_mutation.set(fader_saved);
                    glob_texture.set(shift_fader_saved);
                    glob_alpha.set(alpha_saved);
                    glob_octaves.set(clamp_octaves(octaves));
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
                    } else {
                        leds.set(
                            0,
                            Led::Button,
                            octave_color(glob_octaves.get()),
                            LED_BRIGHTNESS,
                        );
                    }
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

            // Layer LED feedback for Alt (texture) / Third (Lévy α).
            match latch_active_layer {
                LatchLayer::Alt => {
                    let t = glob_texture.get();
                    leds.set(
                        0,
                        Led::Top,
                        Color::Orange,
                        Brightness::Custom((t / 16) as u8),
                    );
                }
                LatchLayer::Third => {
                    let a = glob_alpha.get();
                    leds.set(
                        0,
                        Led::Top,
                        Color::Violet,
                        Brightness::Custom((a / 16) as u8),
                    );
                }
                LatchLayer::Main => {}
            }

            // Reverse gesture feedback (white↔off).
            let fade_left = glob_reverse_fade.get();
            if fade_left > 0 {
                let elapsed = REVERSE_FADE_MS.saturating_sub(fade_left);
                let bright = if glob_reverse_fade_up.get() {
                    ((elapsed as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                } else {
                    (((REVERSE_FADE_MS - elapsed) as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                };
                leds.set(0, Led::Button, Color::White, Brightness::Custom(bright));
                let next = fade_left.saturating_sub(1);
                glob_reverse_fade.set(next);
                if next == 0 && !glob_muted.get() {
                    leds.set(
                        0,
                        Led::Button,
                        octave_color(glob_octaves.get()),
                        LED_BRIGHTNESS,
                    );
                } else if next == 0 && glob_muted.get() {
                    leds.unset(0, Led::Button);
                }
            }
        }
    };

    join(
        long_press,
        join5(fut_clock, fut_buttons, fut_faders, scene_handler, shift),
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
    raw: u8,
    note_on: &mut Option<MidiNote>,
) {
    // Quantize via 1V/oct counts derived from the MIDI note, then emit both
    // CV and MIDI (same dual-path idea as GenSeq / Golden Gate pitch modes).
    let pitch = note_to_pitch(raw);
    let counts = pitch.as_counts(out_range, vpo);
    let q = quantizer.get_quantized_note(counts).await;
    let out_counts = q.as_counts(out_range, vpo);
    if let Some(jack) = cv_jack {
        jack.set_value(out_counts);
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
    midi.send_note_on(n, 4095).await;
    *note_on = Some(n);
    leds.set(0, Led::Bottom, led_color, Brightness::High);
}
