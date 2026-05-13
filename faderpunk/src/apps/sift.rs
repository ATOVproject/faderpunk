//! Sift is a two-channel threshold sequencer:
//!
//! - Channel 0 is the gate lane. Its main fader controls gate density, the
//!   output jack emits gates, and the top LED shows gate activity.
//! - Channel 1 is the CV lane. Its main fader chooses the playback resolution
//!   from a small musical set of CV level counts, the output jack emits CV, and
//!   the top LED mirrors the current CV level.
//!
//! Each lane has ten independently selectable stored patterns. The active gate
//! pattern is selected with Shift + Fader 0; the active CV pattern is selected
//! with Shift + Fader 1. Selection changes flash the corresponding bottom LED
//! once. The main bottom LEDs continue to show gate density and CV level depth.
//!
//! Playback is clocked from the shared 24 PPQN transport. Gate density is
//! sampled at step boundaries so moving the fader cannot create an off-clock
//! trigger. A passing gate step raises the gate jack and emits a MIDI note
//! derived from the current CV through Faderpunk's global quantizer. The note
//! is released with the gate, on reset, on stop, when muted, or when a scene is
//! loaded.
//!
//! Buttons:
//! - Fn 0 resets both lane positions to the next first step.
//! - Fn 1 toggles mute; both button LEDs dim while muted.
//! - Shift + Fn 0 randomizes the currently selected gate and CV patterns.
//! - Shift + Fn 1 randomizes every stored gate and CV pattern.
//!
//! Scenes persist pattern seeds, selected pattern numbers, live fader values,
//! and mute state. Patterns are regenerated deterministically from compact
//! seeds instead of storing every 32-step cell, keeping the complete pattern
//! bank within the app storage budget while preserving exact scene recall.
use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{scale_bits_12_8, value_to_index},
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Param,
    Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};
use crate::tasks::leds::LedMode;

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 11;

const NUM_SEQUENCES: usize = 10;
const MAX_STEPS: usize = 32;
const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const MUTED_BRIGHTNESS: Brightness = Brightness::Low;

const POLARITY_NAMES: &[&str] = &["Positive", "Bipolar", "Negative"];
const CLOCK_DIV_NAMES: &[&str] = &[
    "4 bars", "2 bars", "1 bar", "1/2", "1/4", "1/8T", "1/8", "1/16T", "1/16",
    "1/32T", "1/32", "1/64",
];
const CLOCK_DIVS: &[u32] = &[384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];
const CV_LEVELS: &[u16] = &[2, 3, 4, 5, 6, 8, 12, 16, 24, 32];

#[derive(Clone, Copy)]
enum Polarity {
    Positive,
    Bipolar,
    Negative,
}

impl Polarity {
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Positive,
            2 => Self::Negative,
            _ => Self::Bipolar,
        }
    }
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Sift",
    "Threshold and level sequencer",
    Color::Rose,
    AppIcon::SequenceSquare,
)
.add_param(Param::i32 {
    name: "CV Steps",
    min: 1,
    max: MAX_STEPS as i32,
})
.add_param(Param::i32 {
    name: "Gate Steps",
    min: 1,
    max: MAX_STEPS as i32,
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Polarity",
    variants: POLARITY_NAMES,
})
.add_param(Param::i32 {
    name: "Min CV x0.1V",
    min: -50,
    max: 100,
})
.add_param(Param::i32 {
    name: "Max CV x0.1V",
    min: -50,
    max: 100,
})
.add_param(Param::Enum {
    name: "Clock Division",
    variants: CLOCK_DIV_NAMES,
})
.add_param(Param::i32 {
    name: "Gate Length %",
    min: 5,
    max: 95,
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
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiOut);

pub struct Params {
    cv_steps: i32,
    gate_steps: i32,
    range: Range,
    polarity: usize,
    min_cv_tenths: i32,
    max_cv_tenths: i32,
    clock_div: usize,
    gate_length: i32,
    color: Color,
    midi_channel: MidiChannel,
    midi_out: MidiOut,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }

        Some(Self {
            cv_steps: i32::from_value(values[0]).clamp(1, MAX_STEPS as i32),
            gate_steps: i32::from_value(values[1]).clamp(1, MAX_STEPS as i32),
            range: Range::from_value(values[2]),
            polarity: usize::from_value(values[3]).min(POLARITY_NAMES.len() - 1),
            min_cv_tenths: i32::from_value(values[4]).clamp(-50, 100),
            max_cv_tenths: i32::from_value(values[5]).clamp(-50, 100),
            clock_div: usize::from_value(values[6]).min(CLOCK_DIVS.len() - 1),
            gate_length: i32::from_value(values[7]).clamp(5, 95),
            color: Color::from_value(values[8]),
            midi_channel: MidiChannel::from_value(values[9]),
            midi_out: MidiOut::from_value(values[10]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.cv_steps.into()).unwrap();
        vec.push(self.gate_steps.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.polarity.into()).unwrap();
        vec.push(self.min_cv_tenths.into()).unwrap();
        vec.push(self.max_cv_tenths.into()).unwrap();
        vec.push(self.clock_div.into()).unwrap();
        vec.push(self.gate_length.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    cv_seeds: [u16; NUM_SEQUENCES],
    gate_seeds: [u16; NUM_SEQUENCES],
    selected_cv: u8,
    selected_gate: u8,
    threshold: u16,
    cv_levels_fader: u16,
    cv_select_fader: u16,
    gate_select_fader: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        let mut cv_seeds = [0; NUM_SEQUENCES];
        let mut gate_seeds = [0; NUM_SEQUENCES];
        let mut i = 0;

        while i < NUM_SEQUENCES {
            cv_seeds[i] = 0x0471_u16.wrapping_add((i as u16).wrapping_mul(0x0199));
            gate_seeds[i] = 0x0a3d_u16.wrapping_add((i as u16).wrapping_mul(0x012d));
            i += 1;
        }

        Self {
            cv_seeds,
            gate_seeds,
            selected_cv: 0,
            selected_gate: 0,
            threshold: 2048,
            cv_levels_fader: 2048,
            cv_select_fader: 0,
            gate_select_fader: 0,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id, Params {
        cv_steps: 16,
        gate_steps: 16,
        range: Range::_0_5V,
        polarity: 0,
        min_cv_tenths: 0,
        max_cv_tenths: 50,
        clock_div: 8,
        gate_length: 50,
        color: Color::Rose,
        midi_channel: MidiChannel::default(),
        midi_out: MidiOut::default(),
    });
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
        cv_steps,
        gate_steps,
        range,
        polarity,
        min_cv,
        max_cv,
        clock_div_idx,
        gate_length,
        led_color,
        midi_channel,
        midi_out,
    ) =
        params.query(|p| {
            (
                p.cv_steps.clamp(1, MAX_STEPS as i32) as usize,
                p.gate_steps.clamp(1, MAX_STEPS as i32) as usize,
                p.range,
                Polarity::from_index(p.polarity),
                p.min_cv_tenths,
                p.max_cv_tenths,
                p.clock_div.min(CLOCK_DIVS.len() - 1),
                p.gate_length.clamp(5, 95) as u32,
                p.color,
                p.midi_channel,
                p.midi_out,
            )
        });

    let clock_div = CLOCK_DIVS[clock_div_idx];
    let gate = app.make_gate_jack(0, 4095).await;
    let output = app.make_out_jack(1, range).await;
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let die = app.use_die();
    let quantizer = app.use_quantizer(range, VoltPerOct::Standard, false);
    let midi = app.use_midi_output(midi_out, midi_channel, false);
    let latch_layer = app.make_global(LatchLayer::Main);
    let reset_requested = app.make_global(false);
    let note_on = app.make_global(false);
    let last_note = app.make_global(MidiNote::default());
    let active_gate_density = app.make_global(storage.query(|s| s.threshold));

    refresh_leds(storage, &leds, led_color);
    gate.set_low().await;

    let clock_handler = async {
        let mut clock = app.use_clock();
        let ticks = clock.get_ticker();
        let mut tick_origin = (ticks() as u32).wrapping_add(1);
        let mut cv_step = reset_step_position(cv_steps);
        let mut gate_step = reset_step_position(gate_steps);
        let mut gate_is_high = false;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Start => {
                    if note_on.get() {
                        midi.send_note_off(last_note.get()).await;
                        note_on.set(false);
                    }
                    tick_origin = (ticks() as u32).wrapping_add(1);
                    cv_step = reset_step_position(cv_steps);
                    gate_step = reset_step_position(gate_steps);
                    gate_is_high = false;
                    active_gate_density.set(storage.query(|s| s.threshold));
                    gate.set_low().await;
                    leds.unset(1, Led::Top);
                    leds.unset(0, Led::Top);
                }
                ClockEvent::Stop => {
                    if note_on.get() {
                        midi.send_note_off(last_note.get()).await;
                        note_on.set(false);
                    }
                    gate_is_high = false;
                    gate.set_low().await;
                    leds.unset(0, Led::Top);
                }
                ClockEvent::Tick => {
                    if reset_requested.get() {
                        if note_on.get() {
                            midi.send_note_off(last_note.get()).await;
                            note_on.set(false);
                        }
                        reset_requested.set(false);
                        tick_origin = ticks() as u32;
                        cv_step = reset_step_position(cv_steps);
                        gate_step = reset_step_position(gate_steps);
                        gate_is_high = false;
                        active_gate_density.set(storage.query(|s| s.threshold));
                        gate.set_low().await;
                        leds.unset(0, Led::Top);
                    }

                    let clkn = (ticks() as u32).wrapping_sub(tick_origin);
                    let gate_off_tick = gate_length_ticks(clock_div, gate_length);

                    if clkn.is_multiple_of(clock_div) {
                        cv_step = (cv_step + 1) % cv_steps;
                        gate_step = (gate_step + 1) % gate_steps;

                        let (cv_seed, gate_seed, next_density, levels_fader, muted) =
                            storage.query(|s| {
                                let cv_idx = (s.selected_cv as usize).min(NUM_SEQUENCES - 1);
                                let gate_idx = (s.selected_gate as usize).min(NUM_SEQUENCES - 1);
                                (
                                    s.cv_seeds[cv_idx],
                                    s.gate_seeds[gate_idx],
                                    s.threshold,
                                    s.cv_levels_fader,
                                    s.muted,
                                )
                            });

                        let raw_cv = sequence_cell(cv_seed, cv_step, 0x3141);
                        let levels = cv_levels_from_fader(levels_fader);
                        let cv = quantized_cv(raw_cv, levels, range, polarity, min_cv, max_cv);
                        output.set_value(cv);
                        leds.set(
                            1,
                            Led::Top,
                            led_color,
                            Brightness::Custom(scale_bits_12_8(cv)),
                        );

                        if gate_is_high {
                            if note_on.get() {
                                midi.send_note_off(last_note.get()).await;
                                note_on.set(false);
                            }
                            gate.set_low().await;
                            gate_is_high = false;
                        }

                        let gate_cell = sequence_cell(gate_seed, gate_step, 0x2718);
                        if gate_passes(gate_cell, active_gate_density.get()) && !muted {
                            let note = quantizer.get_quantized_note(cv).await.as_midi();
                            gate.set_high().await;
                            gate_is_high = true;
                            last_note.set(note);
                            note_on.set(true);
                            midi.send_note_on(note, 4095).await;
                            leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                        } else {
                            leds.unset(0, Led::Top);
                        }

                        active_gate_density.set(next_density);
                        refresh_button_leds(storage, &leds, led_color);
                    }

                    if gate_is_high && clkn % clock_div == gate_off_tick {
                        if note_on.get() {
                            midi.send_note_off(last_note.get()).await;
                            note_on.set(false);
                        }
                        gate.set_low().await;
                        gate_is_high = false;
                        leds.unset(0, Led::Top);
                    }
                }
            }
        }
    };

    let fader_handler = async {
        let mut latches = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let layer = latch_layer.get();
            let target_value = match (chan, layer) {
                (0, LatchLayer::Main) => storage.query(|s| s.threshold),
                (0, LatchLayer::Alt) => storage.query(|s| s.gate_select_fader),
                (1, LatchLayer::Main) => storage.query(|s| s.cv_levels_fader),
                (1, LatchLayer::Alt) => storage.query(|s| s.cv_select_fader),
                _ => 0,
            };

            if let Some(new_value) =
                latches[chan].update(faders.get_value_at(chan), layer, target_value)
            {
                match (chan, layer) {
                    (0, LatchLayer::Main) => {
                        storage.modify_and_save(|s| s.threshold = new_value);
                        leds.set(
                            0,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom(scale_bits_12_8(new_value)),
                        );
                    }
                    (0, LatchLayer::Alt) => {
                        let selected = value_to_index(new_value, NUM_SEQUENCES) as u8;
                        let changed = storage.query(|s| s.selected_gate != selected);
                        storage.modify_and_save(|s| {
                            s.gate_select_fader = new_value;
                            s.selected_gate = selected;
                        });
                        if changed {
                            flash_selection_led(storage, &leds, 0, Color::Blue, led_color);
                        }
                    }
                    (1, LatchLayer::Main) => {
                        storage.modify_and_save(|s| s.cv_levels_fader = new_value);
                        leds.set(
                            1,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom(level_led_brightness(new_value)),
                        );
                    }
                    (1, LatchLayer::Alt) => {
                        let selected = value_to_index(new_value, NUM_SEQUENCES) as u8;
                        let changed = storage.query(|s| s.selected_cv != selected);
                        storage.modify_and_save(|s| {
                            s.selected_cv = selected;
                            s.cv_select_fader = new_value;
                        });
                        if changed {
                            flash_selection_led(storage, &leds, 1, Color::Blue, led_color);
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;

            match (chan, shift) {
                (0, false) => {
                    reset_requested.set(true);
                    if note_on.get() {
                        midi.send_note_off(last_note.get()).await;
                        note_on.set(false);
                    }
                    gate.set_low().await;
                    leds.unset(0, Led::Top);
                }
                (1, false) => {
                    let muted = storage.modify_and_save(|s| {
                        s.muted = !s.muted;
                        s.muted
                    });

                    if muted {
                        if note_on.get() {
                            midi.send_note_off(last_note.get()).await;
                            note_on.set(false);
                        }
                        gate.set_low().await;
                        refresh_button_leds(storage, &leds, led_color);
                        leds.unset(0, Led::Top);
                    } else {
                        refresh_button_leds(storage, &leds, led_color);
                    }
                }
                (0, true) => {
                    randomize_active(storage, die);
                    refresh_leds(storage, &leds, led_color);
                }
                (1, true) => {
                    randomize_all(storage, die);
                    refresh_leds(storage, &leds, led_color);
                }
                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    reset_requested.set(true);
                    if note_on.get() {
                        midi.send_note_off(last_note.get()).await;
                        note_on.set(false);
                    }
                    gate.set_low().await;
                    refresh_leds(storage, &leds, led_color);
                    leds.unset(1, Led::Top);
                    leds.unset(0, Led::Top);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let shift_handler = async {
        loop {
            app.delay_millis(1).await;
            latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));
        }
    };

    join5(
        clock_handler,
        fader_handler,
        button_handler,
        scene_handler,
        shift_handler,
    )
    .await;
}

fn randomize_active(storage: &ManagedStorage<Storage>, die: crate::app::Die) {
    storage.modify_and_save(|s| {
        let cv_idx = (s.selected_cv as usize).min(NUM_SEQUENCES - 1);
        let gate_idx = (s.selected_gate as usize).min(NUM_SEQUENCES - 1);
        s.cv_seeds[cv_idx] = random_seed(die);
        s.gate_seeds[gate_idx] = random_seed(die);
    });
}

fn randomize_all(storage: &ManagedStorage<Storage>, die: crate::app::Die) {
    storage.modify_and_save(|s| {
        for seed in &mut s.cv_seeds {
            *seed = random_seed(die);
        }

        for seed in &mut s.gate_seeds {
            *seed = random_seed(die);
        }
    });
}

fn random_seed(die: crate::app::Die) -> u16 {
    die.roll() ^ die.roll().rotate_left(4)
}

fn refresh_leds(storage: &ManagedStorage<Storage>, leds: &crate::app::Leds<CHANNELS>, color: Color) {
    refresh_button_leds(storage, leds, color);

    let (threshold, levels) = bottom_led_brightness(storage);
    leds.set(0, Led::Bottom, color, Brightness::Custom(threshold));
    leds.set(1, Led::Bottom, color, Brightness::Custom(levels));
}

fn bottom_led_brightness(storage: &ManagedStorage<Storage>) -> (u8, u8) {
    let (threshold, levels) = storage.query(|s| (s.threshold, s.cv_levels_fader));
    (scale_bits_12_8(threshold), level_led_brightness(levels))
}

fn flash_selection_led(
    storage: &ManagedStorage<Storage>,
    leds: &crate::app::Leds<CHANNELS>,
    channel: usize,
    flash_color: Color,
    restore_color: Color,
) {
    let (threshold, levels) = bottom_led_brightness(storage);
    let restore_brightness = if channel == 0 { threshold } else { levels };
    leds.set_mode(
        channel,
        Led::Bottom,
        LedMode::FlashThenStatic(
            flash_color,
            1,
            restore_color,
            Brightness::Custom(restore_brightness),
        ),
    );
}

fn refresh_button_leds(
    storage: &ManagedStorage<Storage>,
    leds: &crate::app::Leds<CHANNELS>,
    color: Color,
) {
    let brightness = if storage.query(|s| s.muted) {
        MUTED_BRIGHTNESS
    } else {
        LED_BRIGHTNESS
    };

    leds.set(0, Led::Button, color, brightness);
    leds.set(1, Led::Button, color, brightness);
}

fn reset_step_position(length: usize) -> usize {
    length.clamp(1, MAX_STEPS) - 1
}

fn sequence_cell(seed: u16, step: usize, salt: u16) -> u16 {
    let mut x = seed as u32;
    x ^= (step as u32).wrapping_mul(0x9e37);
    x ^= (salt as u32) << 8;
    x ^= x >> 7;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    ((x ^ (x >> 15)) & 0x0fff) as u16
}

fn cv_levels_from_fader(value: u16) -> u16 {
    CV_LEVELS[value_to_index(value, CV_LEVELS.len())]
}

fn level_led_brightness(value: u16) -> u8 {
    let levels = cv_levels_from_fader(value);
    ((levels as u32 * 255) / 32).clamp(0, 255) as u8
}

fn gate_length_ticks(div: u32, percent: u32) -> u32 {
    (div * percent.clamp(5, 95) / 100).clamp(1, div.saturating_sub(1).max(1))
}

fn gate_passes(gate_cell: u16, density: u16) -> bool {
    gate_cell <= density.min(4095)
}

fn quantized_cv(
    raw: u16,
    levels: u16,
    range: Range,
    polarity: Polarity,
    min_cv_tenths: i32,
    max_cv_tenths: i32,
) -> u16 {
    let (mut min_cv, mut max_cv) = effective_cv_bounds(range, polarity, min_cv_tenths, max_cv_tenths);

    if min_cv > max_cv {
        core::mem::swap(&mut min_cv, &mut max_cv);
    }

    let min_count = volts_to_counts(min_cv, range);
    let max_count = volts_to_counts(max_cv, range);
    let levels = levels.max(2) as u32;
    let index = (raw as u32 * (levels - 1) + 2047) / 4095;

    if min_count == max_count {
        return min_count;
    }

    let (low, high) = if min_count <= max_count {
        (min_count, max_count)
    } else {
        (max_count, min_count)
    };
    low + (((high - low) as u32 * index) / (levels - 1)) as u16
}

fn effective_cv_bounds(
    range: Range,
    polarity: Polarity,
    min_cv_tenths: i32,
    max_cv_tenths: i32,
) -> (i32, i32) {
    let (hw_min, hw_max) = hardware_bounds(range);
    let mut min_cv = min_cv_tenths.min(max_cv_tenths).clamp(hw_min, hw_max);
    let mut max_cv = min_cv_tenths.max(max_cv_tenths).clamp(hw_min, hw_max);

    match polarity {
        Polarity::Positive => {
            min_cv = min_cv.max(0);
            max_cv = max_cv.max(0);
        }
        Polarity::Negative => {
            min_cv = min_cv.min(0);
            max_cv = max_cv.min(0);
        }
        Polarity::Bipolar => {}
    }

    (min_cv.clamp(hw_min, hw_max), max_cv.clamp(hw_min, hw_max))
}

fn hardware_bounds(range: Range) -> (i32, i32) {
    match range {
        Range::_0_10V => (0, 100),
        Range::_0_5V => (0, 50),
        Range::_Neg5_5V => (-50, 50),
    }
}

fn volts_to_counts(volts_tenths: i32, range: Range) -> u16 {
    match range {
        Range::_0_10V => (volts_tenths.clamp(0, 100) as u32 * 4095 / 100) as u16,
        Range::_0_5V => (volts_tenths.clamp(0, 50) as u32 * 4095 / 50) as u16,
        Range::_Neg5_5V => (((volts_tenths.clamp(-50, 50) + 50) as u32) * 4095 / 100) as u16,
    }
}
