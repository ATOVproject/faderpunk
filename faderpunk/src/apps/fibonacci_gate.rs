use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config, MidiCc,
    MidiChannel, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 7;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Reverse gesture LED feedback length (white↔off fade), same as Heat Pump invert.
const REVERSE_FADE_MS: u16 = 500;

/// Fibonacci gaps (in steps) between consecutive hits.
const FIB: [u16; 10] = [1, 1, 2, 3, 5, 8, 13, 21, 34, 55];
const MIN_CYCLE: u32 = 8;
const MAX_CYCLE: u32 = 32;
/// Pitch mode: cap intervals to two octaves.
const MAX_SEMIS: u16 = 24;

/// Output modes, cycled on the device via shift + long press.
const MODE_GATE_NOTE: u8 = 0;
const MODE_GATE_CC: u8 = 1;
const MODE_PITCH: u8 = 2;
/// Pitch CV/MIDI with φ-spaced intervals (≈833¢), not 12-TET.
const MODE_PITCH_PHI: u8 = 3;

/// ≈1200·log₂(φ) cents — one golden-ratio frequency step.
const PHI_CENTS: u32 = 833;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Golden Gate",
    "Fibonacci-spaced gates — successive ratios approach φ",
    Color::Violet,
    AppIcon::SequenceSquare,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "MIDI Note" })
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
.add_param(Param::Enum {
    name: "Speed",
    variants: &["16th", "8th", "Quarter"],
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
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    midi_out: MidiOut,
    note: MidiNote,
    cc: MidiCc,
    gatel: i32,
    speed: usize,
    color: Color,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            cc: MidiCc::from_value(values[2]),
            gatel: i32::from_value(values[3]),
            speed: usize::from_value(values[4]),
            color: Color::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.cc.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.speed.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Main fader: max Fibonacci depth (raw 12-bit).
    fader_saved: u16,
    /// Shift fader: cycle length N in steps (raw 12-bit).
    shift_fader_saved: u16,
    muted: bool,
    reversed: bool,
    /// 0 = gate+note, 1 = gate+CC, 2 = pitch 12-TET, 3 = pitch φ.
    out_mode: u8,
    /// 0 = 16th, 1 = 8th, 2 = quarter; 255 = follow the Speed param.
    speed_saved: u8,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 2048,
            shift_fader_saved: 2048,
            muted: false,
            reversed: false,
            out_mode: MODE_GATE_NOTE,
            speed_saved: 255,
        }
    }
}
impl AppStorage for Storage {}

fn cycle_from_value(value: u16) -> u32 {
    MIN_CYCLE + value as u32 * (MAX_CYCLE - MIN_CYCLE) / 4095
}

fn depth_from_value(value: u16) -> u32 {
    // 2..=FIB.len() gap values in rotation
    2 + value as u32 * (FIB.len() as u32 - 2) / 4095
}

/// Gaps used to fill one cycle of length `cycle` (forward Fibonacci order).
fn cycle_gaps(cycle: u32, depth: u32) -> heapless::Vec<u16, 32> {
    let mut gaps = heapless::Vec::new();
    let depth = depth.max(1) as usize;
    let mut pos = 0u32;
    let mut i = 0usize;
    while pos < cycle {
        let g = FIB[i % depth];
        if gaps.push(g).is_err() {
            break;
        }
        pos += g as u32;
        i += 1;
    }
    gaps
}

/// Precompute the hit mask for one cycle. Forward: cumulative Fibonacci gaps
/// until ≥ N. Reverse: the *same* gaps that filled this cycle, in reverse
/// order (not FIB[depth-1]…0 from the depth window — that ignored N).
fn build_mask(cycle: u32, depth: u32, reversed: bool) -> u32 {
    let mut gaps = cycle_gaps(cycle, depth);
    if reversed {
        gaps.reverse();
    }
    let mut mask = 0u32;
    let mut pos = 0u32;
    for &g in gaps.iter() {
        if pos >= cycle {
            break;
        }
        mask |= 1 << pos;
        pos += g as u32;
    }
    mask
}

/// Gap at a hit step (for pitch modes), matching `build_mask`.
fn gap_at_step(step: u32, cycle: u32, depth: u32, reversed: bool) -> u16 {
    let mut gaps = cycle_gaps(cycle, depth);
    if reversed {
        gaps.reverse();
    }
    let mut pos = 0u32;
    for &g in gaps.iter() {
        if pos == step {
            return g.min(MAX_SEMIS);
        }
        if pos > step {
            break;
        }
        pos += g as u32;
    }
    1
}

/// 1V/oct at 0-10V range: full scale is 120 semitones / 12000 cents.
fn semis_to_counts(semis: u16) -> u16 {
    (semis as u32 * 4095 / 120) as u16
}

fn cents_to_counts(cents: u32) -> u16 {
    (cents * 4095 / 12_000).min(4095) as u16
}

/// Nearest MIDI note + pitch bend (±2 semitone range assumed) for a
/// cent offset above `base`.
fn note_and_bend(base: MidiNote, cents_offset: u32) -> (MidiNote, u16) {
    let semis = ((cents_offset + 50) / 100) as i8;
    let bend_cents = cents_offset as i32 - semis as i32 * 100;
    let n = { base }.transpose(semis);
    // ±200¢ ↔ full 14-bit bend (±2 semitone synth range)
    let bend = (8192i32 + bend_cents * 8192 / 200).clamp(0, 16_383) as u16;
    (n, bend)
}

fn is_pitch_mode(mode: u8) -> bool {
    mode == MODE_PITCH || mode == MODE_PITCH_PHI
}

/// Ticks per step at 24 PPQN: 16th / 8th / quarter.
fn div_for_speed(speed: u8) -> u32 {
    match speed {
        1 => 12,
        2 => 24,
        _ => 6,
    }
}

fn mode_color(mode: u8, led_color: Color) -> Color {
    match mode {
        MODE_GATE_CC => Color::Orange,
        MODE_PITCH => Color::Red,
        MODE_PITCH_PHI => Color::Pink,
        _ => led_color,
    }
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut([false, false, false]),
            note: MidiNote::from(36),
            cc: MidiCc::default(),
            gatel: 50,
            speed: 0,
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
    let (midi_out, midi_chan, note, cc, gatel, param_speed, led_color) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.note,
            p.cc,
            p.gatel as u32,
            p.speed,
            p.color,
        )
    });

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let glob_muted = app.make_global(false);
    let glob_reversed = app.make_global(false);
    let glob_cycle = app.make_global(16_u32);
    let glob_depth = app.make_global(5_u32);
    let glob_mask = app.make_global(0_u32);
    let glob_mode = app.make_global(MODE_GATE_NOTE);
    let glob_speed = app.make_global(0_u8);
    let glob_reset = app.make_global(false);
    let long_press_fired = app.make_global(false);
    let glob_fader_moved = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    // Remaining ms of reverse LED fade; 0 = inactive.
    let glob_reverse_fade = app.make_global(0u16);
    // true = none→white, false = white→none.
    let glob_reverse_fade_up = app.make_global(false);

    let (fader_saved, shift_fader_saved, muted, reversed, out_mode, speed_saved) =
        storage.query(|s| {
            (
                s.fader_saved,
                s.shift_fader_saved,
                s.muted,
                s.reversed,
                s.out_mode,
                s.speed_saved,
            )
        });

    // Both handles address the same port; the hardware mode (GPO vs DAC)
    // follows whichever make_* call ran last. Reconfigured on mode toggle.
    let cv_jack = app.make_out_jack(0, Range::_0_10V).await;
    let gate_jack = app.make_gate_jack(0, 4095).await;
    if is_pitch_mode(out_mode) {
        app.make_out_jack(0, Range::_0_10V).await;
        cv_jack.set_value(0);
    }

    // make_gate_jack drives the port high on configure; force a known-off
    // state (also clears any note left sounding by a prior run() that was
    // dropped mid-gate, e.g. on a param change respawn).
    midi.send_note_off(note).await;
    if is_pitch_mode(out_mode) {
        midi.send_pitch_bend(8192).await;
    }
    if !is_pitch_mode(out_mode) {
        gate_jack.set_low().await;
    }

    glob_muted.set(muted);
    glob_reversed.set(reversed);
    glob_mode.set(out_mode);
    glob_speed.set(if speed_saved <= 2 {
        speed_saved
    } else {
        param_speed as u8
    });
    glob_depth.set(depth_from_value(fader_saved));
    glob_cycle.set(cycle_from_value(shift_fader_saved));
    glob_mask.set(build_mask(
        glob_cycle.get(),
        glob_depth.get(),
        glob_reversed.get(),
    ));

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(
            0,
            Led::Button,
            mode_color(out_mode, led_color),
            LED_BRIGHTNESS,
        );
    }

    let fut_clock = async {
        let mut note_on: Option<MidiNote> = None;
        let mut cc_on = false;
        let mut cached_mode = glob_mode.get();
        let mut step = 0u32;
        let mut cached_speed = glob_speed.get();
        let mut div = div_for_speed(cached_speed);
        let mut gate_step = (div * gatel / 100).clamp(1, div - 1);

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Stop => {
                    if let Some(n) = note_on.take() {
                        midi.send_note_off(n).await;
                    }
                    if is_pitch_mode(cached_mode) {
                        midi.send_pitch_bend(8192).await;
                    }
                    if cc_on {
                        midi.send_cc(cc, 0).await;
                        cc_on = false;
                    }
                    step = 0;
                    glob_reset.set(false);
                    if !is_pitch_mode(cached_mode) {
                        gate_jack.set_low().await;
                    }
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                }
                ClockEvent::Tick => {
                    let clkn = ticks() as u32;

                    // Mode changed on the device: reconfigure the jack.
                    let mode = glob_mode.get();
                    if mode != cached_mode {
                        if is_pitch_mode(mode) {
                            app.make_out_jack(0, Range::_0_10V).await;
                            cv_jack.set_value(0);
                        } else {
                            app.make_gate_jack(0, 4095).await;
                            gate_jack.set_low().await;
                        }
                        // Leaving φ mode: recenter any pitch bend left behind.
                        if cached_mode == MODE_PITCH_PHI {
                            midi.send_pitch_bend(8192).await;
                        }
                        cached_mode = mode;
                    }

                    let speed = glob_speed.get();
                    if speed != cached_speed {
                        cached_speed = speed;
                        div = div_for_speed(speed);
                        gate_step = (div * gatel / 100).clamp(1, div - 1);
                    }

                    if clkn.is_multiple_of(div) {
                        if glob_reset.get() {
                            glob_reset.set(false);
                            step = 0;
                        }

                        let cycle = glob_cycle.get();
                        if step >= cycle {
                            step = 0;
                        }

                        let hit = glob_mask.get() & (1 << step) != 0;
                        if hit && !glob_muted.get() {
                            match cached_mode {
                                MODE_GATE_CC => {
                                    gate_jack.set_high().await;
                                    midi.send_cc(cc, 4095).await;
                                    cc_on = true;
                                }
                                MODE_PITCH => {
                                    let semis = gap_at_step(
                                        step,
                                        glob_cycle.get(),
                                        glob_depth.get(),
                                        glob_reversed.get(),
                                    );
                                    cv_jack.set_value(semis_to_counts(semis));
                                    let n = { note }.transpose(semis as i8);
                                    midi.send_note_on(n, 4095).await;
                                    note_on = Some(n);
                                }
                                MODE_PITCH_PHI => {
                                    let gap = gap_at_step(
                                        step,
                                        glob_cycle.get(),
                                        glob_depth.get(),
                                        glob_reversed.get(),
                                    );
                                    let cents = (gap as u32 * PHI_CENTS).min(2400);
                                    cv_jack.set_value(cents_to_counts(cents));
                                    let (n, bend) = note_and_bend(note, cents);
                                    midi.send_pitch_bend(bend).await;
                                    midi.send_note_on(n, 4095).await;
                                    note_on = Some(n);
                                }
                                _ => {
                                    gate_jack.set_high().await;
                                    midi.send_note_on(note, 4095).await;
                                    note_on = Some(note);
                                }
                            }
                            leds.set(0, Led::Bottom, led_color, Brightness::High);
                        }

                        step += 1;
                    }

                    if clkn % div == gate_step {
                        if let Some(n) = note_on.take() {
                            midi.send_note_off(n).await;
                            if cached_mode == MODE_PITCH_PHI {
                                midi.send_pitch_bend(8192).await;
                            }
                        }
                        if cc_on {
                            midi.send_cc(cc, 0).await;
                            cc_on = false;
                        }
                        if !is_pitch_mode(cached_mode) {
                            gate_jack.set_low().await;
                        }
                        leds.set(0, Led::Bottom, led_color, Brightness::Off);
                    }

                    // Top LED: bar progress by default; Fibonacci depth while
                    // Shift is held, speed while the button is held — same
                    // "preview whatever the fader is adjusting" convention as
                    // Heat Pump / Grooves. Runs every tick (not just on hit
                    // steps) so it tracks fader moves immediately.
                    match glob_latch_layer.get() {
                        LatchLayer::Main => {
                            let cycle = glob_cycle.get().max(1);
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom(((step % cycle) * 255 / cycle) as u8),
                            );
                        }
                        LatchLayer::Alt => {
                            // cycle ranges MIN_CYCLE..=MAX_CYCLE — normalize to 0..255.
                            let cyc = glob_cycle.get();
                            let span = (MAX_CYCLE - MIN_CYCLE).max(1);
                            let norm = ((cyc.saturating_sub(MIN_CYCLE)) * 255 / span) as u8;
                            leds.set(0, Led::Top, Color::Red, Brightness::Custom(norm));
                        }
                        LatchLayer::Third => {
                            let speed_color = match glob_speed.get() {
                                0 => Color::Cyan,   // 16th
                                1 => Color::Yellow, // 8th
                                _ => Color::Orange, // quarter
                            };
                            leds.set(0, Led::Top, speed_color, LED_BRIGHTNESS);
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
                    // Shift + short press: reverse direction through the list.
                    let reversed = glob_reversed.toggle();
                    storage.modify_and_save(|s| s.reversed = reversed);
                    glob_mask.set(build_mask(glob_cycle.get(), glob_depth.get(), reversed));
                    // Reverse on → white→none; reverse off → none→white.
                    glob_reverse_fade_up.set(!reversed);
                    glob_reverse_fade.set(REVERSE_FADE_MS);
                }
            } else {
                long_press_fired.set(false);
                glob_fader_moved.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    // Short press: reset the sequence to the downbeat.
                    glob_reset.set(true);
                } else if !glob_fader_moved.get() {
                    // Long press (without moving the fader): toggle mute.
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| s.muted = muted);
                    if muted {
                        midi.send_note_off(note).await;
                        midi.send_cc(cc, 0).await;
                        if !is_pitch_mode(glob_mode.get()) {
                            gate_jack.set_low().await;
                        }
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
                // Shift + long press: cycle output mode (Note / CC / Pitch).
                let mode = (glob_mode.get() + 1) % 4;
                glob_mode.set(mode);
                storage.modify_and_save(|s| s.out_mode = mode);
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

            // Any fader movement while the button is held counts, even before
            // the latch picks up — otherwise releasing would still mute.
            if latch_layer == LatchLayer::Third {
                glob_fader_moved.set(true);
            }

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.fader_saved),
                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved),
                // Center of the current speed's fader zone (up = 16th, down = quarter)
                LatchLayer::Third => (2 - glob_speed.get()) as u16 * 1365 + 683,
            };

            if let Some(new_value) = latch.update(faders.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_depth.set(depth_from_value(new_value));
                        glob_mask.set(build_mask(
                            glob_cycle.get(),
                            glob_depth.get(),
                            glob_reversed.get(),
                        ));
                        storage.modify_and_save(|s| s.fader_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        glob_cycle.set(cycle_from_value(new_value));
                        glob_mask.set(build_mask(
                            glob_cycle.get(),
                            glob_depth.get(),
                            glob_reversed.get(),
                        ));
                        storage.modify_and_save(|s| s.shift_fader_saved = new_value);
                    }
                    LatchLayer::Third => {
                        // Button held + fader: pick speed (up = 16th, down = quarter).
                        glob_fader_moved.set(true);
                        let speed = (2u8).saturating_sub((new_value / 1366).min(2) as u8);
                        if speed != glob_speed.get() {
                            glob_speed.set(speed);
                            storage.modify_and_save(|s| s.speed_saved = speed);
                        }
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
                    let (fader_saved, shift_fader_saved, muted, reversed, out_mode, speed_saved) =
                        storage.query(|s| {
                            (
                                s.fader_saved,
                                s.shift_fader_saved,
                                s.muted,
                                s.reversed,
                                s.out_mode,
                                s.speed_saved,
                            )
                        });

                    glob_muted.set(muted);
                    glob_reversed.set(reversed);
                    glob_mode.set(out_mode);
                    if speed_saved <= 2 {
                        glob_speed.set(speed_saved);
                    }
                    glob_depth.set(depth_from_value(fader_saved));
                    glob_cycle.set(cycle_from_value(shift_fader_saved));
                    glob_mask.set(build_mask(
                        glob_cycle.get(),
                        glob_depth.get(),
                        glob_reversed.get(),
                    ));

                    if muted {
                        midi.send_note_off(note).await;
                        midi.send_cc(cc, 0).await;
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(
                            0,
                            Led::Button,
                            mode_color(out_mode, led_color),
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

            // Reverse gesture feedback (white↔off), mirrors Heat Pump invert.
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
