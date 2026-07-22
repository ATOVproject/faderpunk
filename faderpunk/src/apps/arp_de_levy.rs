use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, quantizer::Pitch, AppIcon, Brightness, ClockDivision, Color,
    Config, MidiChannel, MidiNote, MidiOut, Note, Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Die, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 6;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Reverse gesture LED feedback length (white↔off fade), same as Golden Gate / Heat Pump.
const REVERSE_FADE_MS: u16 = 500;

const POOL_CAP: usize = 16;
const MIN_PHRASE: usize = 4;
const MAX_PHRASE: usize = 16;
/// Ticks per 16th at 24 PPQN.
const STEP_DIV: u32 = 6;
/// Heavy-tailed Lévy step table (semitones). Small moves dominate; rare large jumps.
const LEVY_STEPS: [i8; 16] = [1, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 5, 5, 8, 8, 12];

const MODE_UP: u8 = 0;
const MODE_DOWN: u8 = 1;
const MODE_UP_DOWN: u8 = 2;
const MODE_DOWN_UP: u8 = 3;
const MODE_RANDOM: u8 = 4;
const MODE_CONVERGE: u8 = 5;
const MODE_COUNT: u8 = 6;

const OCT_COLORS: [Color; 4] = [Color::Blue, Color::Cyan, Color::Yellow, Color::Red];

pub static CONFIG: Config<PARAMS> = Config::new(
    "Arp de Lévy",
    "Lévy-flight generative arpeggiator — evolving phrase, classic arp modes",
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
});

pub struct Params {
    midi_channel: MidiChannel,
    note: MidiNote,
    color: Color,
    midi_out: MidiOut,
    vpo: VoltPerOct,
    bypass: bool,
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
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            color: Color::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
            vpo: VoltPerOct::from_value(values[4]),
            bypass: bool::from_value(values[5]),
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
        vec
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Storage {
    /// Main fader: mutation rate (raw 12-bit).
    fader_saved: u16,
    /// Shift fader: texture macro (raw 12-bit).
    shift_fader_saved: u16,
    /// Button+fader: octave span 1..=4 (raw 12-bit).
    octave_saved: u16,
    muted: bool,
    reversed: bool,
    /// Classic arp playback mode.
    mode: u8,
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
            octave_saved: 1365, // ~2 octaves
            muted: false,
            reversed: false,
            mode: MODE_UP,
            pool,
            phrase_len: 8,
        }
    }
}

impl AppStorage for Storage {}

fn octaves_from_value(value: u16) -> u8 {
    1 + (value / 1024).min(3) as u8
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

fn mode_color(mode: u8, led_color: Color) -> Color {
    match mode {
        MODE_DOWN => Color::Orange,
        MODE_UP_DOWN => Color::Yellow,
        MODE_DOWN_UP => Color::Lime,
        MODE_RANDOM => Color::Red,
        MODE_CONVERGE => Color::Pink,
        _ => led_color,
    }
}

fn base_midi(note: MidiNote) -> u8 {
    u7::from(note).as_int()
}

fn clamp_note(n: i16) -> u8 {
    n.clamp(0, 127) as u8
}

fn levy_delta(die: &Die) -> i8 {
    let idx = (die.roll() as usize) % LEVY_STEPS.len();
    let mag = LEVY_STEPS[idx];
    if die.roll() & 1 == 0 {
        mag
    } else {
        -mag
    }
}

fn mutate_pool(pool: &mut [u8; POOL_CAP], phrase_len: usize, lo: u8, hi: u8, die: &Die) {
    if phrase_len == 0 {
        return;
    }
    let i = (die.roll() as usize) % phrase_len;
    let next = clamp_note(pool[i] as i16 + levy_delta(die) as i16);
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

/// Map playback step → pool index for the classic arp modes.
fn pool_index(step: usize, len: usize, mode: u8, reversed: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let i = match mode {
        MODE_DOWN => len - 1 - (step % len),
        MODE_UP_DOWN => {
            let cycle = (len * 2).saturating_sub(2).max(1);
            let s = step % cycle;
            if s < len {
                s
            } else {
                cycle - s
            }
        }
        MODE_DOWN_UP => {
            let cycle = (len * 2).saturating_sub(2).max(1);
            let s = step % cycle;
            let up = if s < len { s } else { cycle - s };
            len - 1 - up
        }
        MODE_CONVERGE => {
            let half = step % len;
            if half.is_multiple_of(2) {
                half / 2
            } else {
                len - 1 - half / 2
            }
        }
        // UP and RANDOM resolved by caller (RANDOM picks at fire time).
        _ => step % len,
    };
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
    let (midi_out, midi_chan, base_note, led_color, vpo, bypass) =
        params.query(|p| (p.midi_out, p.midi_channel, p.note, p.color, p.vpo, p.bypass));

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let die = app.use_die();
    let quantizer = app.use_quantizer(Range::_0_10V, vpo, bypass);
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let cv_jack = app.make_out_jack(0, Range::_0_10V).await;

    let (
        fader_saved,
        shift_fader_saved,
        octave_saved,
        muted,
        reversed,
        mode,
        pool_saved,
        phrase_saved,
    ) = storage.query(|s| {
        (
            s.fader_saved,
            s.shift_fader_saved,
            s.octave_saved,
            s.muted,
            s.reversed,
            s.mode,
            s.pool,
            s.phrase_len,
        )
    });

    let glob_muted = app.make_global(muted);
    let glob_reversed = app.make_global(reversed);
    let glob_mode = app.make_global(mode.min(MODE_COUNT - 1));
    let glob_mutation = app.make_global(fader_saved);
    let glob_texture = app.make_global(shift_fader_saved);
    let glob_octave = app.make_global(octave_saved);
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
    cv_jack.set_value(0);

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(
            0,
            Led::Button,
            mode_color(glob_mode.get(), led_color),
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
                    cv_jack.set_value(0);
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                }
                ClockEvent::Tick => {
                    let clkn = ticks() as u32;
                    let octaves = octaves_from_value(glob_octave.get());
                    let lo = base_midi(base_note);
                    let hi = clamp_note(lo as i16 + (octaves as i16) * 12);
                    let (density, phrase_len, swing) = texture_from_value(glob_texture.get());
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
                                        &cv_jack,
                                        &quantizer,
                                        &leds,
                                        led_color,
                                        vpo,
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

                        // At phrase boundary: Lévy-mutate according to mutation rate.
                        if step == 0 {
                            let mut mutation = glob_mutation.get();
                            let mut changed = false;
                            // Number of mutations scales with fader (0 = freeze).
                            while mutation > 0 {
                                if die.roll() < mutation {
                                    mutate_pool(&mut pool, phrase_len, lo, hi, &die);
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

                        let mode = glob_mode.get();
                        let reversed = glob_reversed.get();
                        let idx = if mode == MODE_RANDOM {
                            (die.roll() as usize) % phrase_len.max(1)
                        } else {
                            pool_index(step, phrase_len, mode, reversed)
                        };
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
                                    &cv_jack,
                                    &quantizer,
                                    &leds,
                                    led_color,
                                    vpo,
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
                            mode_color(glob_mode.get(), led_color),
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
                // Shift + long: cycle classic arp mode.
                let mode = (glob_mode.get() + 1) % MODE_COUNT;
                glob_mode.set(mode);
                storage.modify_and_save(|s| s.mode = mode);
                if !glob_muted.get() {
                    leds.set(0, Led::Button, mode_color(mode, led_color), LED_BRIGHTNESS);
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
                LatchLayer::Third => {
                    // Center of current octave zone (1..=4).
                    let oct = octaves_from_value(glob_octave.get()).saturating_sub(1);
                    oct as u16 * 1024 + 512
                }
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
                        glob_octave.set(new_value);
                        storage.modify_and_save(|s| s.octave_saved = new_value);
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
                    let (fader_saved, shift_fader_saved, octave_saved, muted, reversed, mode) =
                        storage.query(|s| {
                            (
                                s.fader_saved,
                                s.shift_fader_saved,
                                s.octave_saved,
                                s.muted,
                                s.reversed,
                                s.mode,
                            )
                        });
                    glob_mutation.set(fader_saved);
                    glob_texture.set(shift_fader_saved);
                    glob_octave.set(octave_saved);
                    glob_muted.set(muted);
                    glob_reversed.set(reversed);
                    glob_mode.set(mode.min(MODE_COUNT - 1));
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
                            mode_color(glob_mode.get(), led_color),
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
        loop {
            app.delay_millis(1).await;
            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);

            // Layer LED feedback for Alt (texture) / Third (octaves).
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
                    let oct = octaves_from_value(glob_octave.get()).saturating_sub(1) as usize;
                    leds.set(0, Led::Top, OCT_COLORS[oct], Brightness::High);
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
                        mode_color(glob_mode.get(), led_color),
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
    cv_jack: &crate::app::OutJack,
    quantizer: &crate::app::Quantizer,
    leds: &crate::app::Leds<CHANNELS>,
    led_color: Color,
    vpo: VoltPerOct,
    raw: u8,
    note_on: &mut Option<MidiNote>,
) {
    // Quantize via 1V/oct counts derived from the MIDI note, then emit both
    // CV and MIDI (same dual-path idea as GenSeq / Golden Gate pitch modes).
    let pitch = note_to_pitch(raw);
    let counts = pitch.as_counts(Range::_0_10V, vpo);
    let q = quantizer.get_quantized_note(counts).await;
    let out_counts = q.as_counts(Range::_0_10V, vpo);
    cv_jack.set_value(out_counts);

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
