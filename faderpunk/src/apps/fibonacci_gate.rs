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
    utils::attenuate_bipolar,
    AppIcon, Brightness, ClockDivision, Color, Config, MidiCc, MidiChannel, MidiNote, MidiOut,
    Param, Range, Value, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 12;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Reverse gesture LED feedback length (white↔off fade), same as Heat Pump invert.
const REVERSE_FADE_MS: u16 = 500;

/// Fibonacci gaps (in steps) between consecutive hits.
const FIB: [u16; 11] = [1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89];
const MIN_CYCLE: u32 = 8;
/// Max phrase length in steps. Hit mask is `u128`, so this must be ≤ 128.
const MAX_CYCLE: u32 = 128;
/// Pitch mode: cap intervals to two octaves.
const MAX_SEMIS: u16 = 24;

/// Same grid as Heat Pump: tick spacing at 24 PPQN, slow → fast.
const SPEED_DIVS: [u32; 10] = [96, 48, 36, 24, 18, 16, 12, 8, 6, 3];
const SPEED_COUNT: u8 = SPEED_DIVS.len() as u8;
/// Default Speed enum index = 16th (div 6).
const SPEED_DEFAULT: u8 = 8;

/// Output modes, cycled on the device via shift + long press.
const MODE_GATE_NOTE: u8 = 0;
const MODE_GATE_CC: u8 = 1;
const MODE_PITCH: u8 = 2;
/// Pitch CV/MIDI with φ-spaced intervals (≈833¢), not 12-TET.
const MODE_PITCH_PHI: u8 = 3;

/// ≈1200·log₂(φ) cents — one golden-ratio frequency step.
const PHI_CENTS: u32 = 833;

const CV_JACK_OUT: usize = 0;
const CV_JACK_IN: usize = 1;
const DEST_DEPTH: usize = 0;
const DEST_CYCLE: usize = 1;
const DEST_RESET: usize = 2;
const DEST_COUNT: usize = 3;
const TRIG_HIGH: u16 = 2458;

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 400) as u32 * 4095) / 100) as u16
}

fn mod_u16(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Golden Gate",
    "Fibonacci-spaced gates - successive ratios approach phi",
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
    // Heat Pump divisions (24 PPQN), slow → fast.
    variants: &[
        "1/1", "1/2", "1/4.", "1/4", "1/8.", "1/4T", "1/8", "1/8T", "1/16", "1/32",
    ],
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
    variants: &["Depth", "Cycle", "Reset"],
})
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 400,
})
.add_param(Param::Enum {
    name: "Mode",
    variants: &["Note", "CC", "Pitch", "Phi"],
});

pub struct Params {
    midi_channel: MidiChannel,
    midi_out: MidiOut,
    note: MidiNote,
    cc: MidiCc,
    gatel: i32,
    speed: usize,
    color: Color,
    cv_jack: usize,
    range: Range,
    cv_dest: usize,
    cv_att: i32,
    /// 0 = Note, 1 = CC, 2 = Pitch 12-TET, 3 = φ pitch.
    mode: usize,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < 7 {
            return None;
        }
        // Indices 7..=10 added with CV jack; 11 = Mode. Accept older layouts.
        let (cv_jack, range, cv_dest, cv_att) = if values.len() >= 11 {
            (
                usize::from_value(values[7]).min(1),
                Range::from_value(values[8]),
                usize::from_value(values[9]).min(DEST_COUNT - 1),
                i32::from_value(values[10]).clamp(0, 400),
            )
        } else {
            (CV_JACK_OUT, Range::_0_10V, DEST_DEPTH, 100)
        };
        let mode = if values.len() >= PARAMS {
            usize::from_value(values[11]).min(3)
        } else {
            MODE_GATE_NOTE as usize
        };
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            cc: MidiCc::from_value(values[2]),
            gatel: i32::from_value(values[3]),
            speed: usize::from_value(values[4]),
            color: Color::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
            cv_jack,
            range,
            cv_dest,
            cv_att,
            mode,
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
        vec.push(self.cv_jack.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.cv_dest.into()).unwrap();
        vec.push(self.cv_att.into()).unwrap();
        vec.push(self.mode.into()).unwrap();
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
    /// Speed index into `SPEED_DIVS` (0..=9); 255 = follow the Speed param.
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
fn cycle_gaps(cycle: u32, depth: u32) -> heapless::Vec<u16, { MAX_CYCLE as usize }> {
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
fn build_mask(cycle: u32, depth: u32, reversed: bool) -> u128 {
    let mut gaps = cycle_gaps(cycle, depth);
    if reversed {
        gaps.reverse();
    }
    let mut mask = 0u128;
    let mut pos = 0u32;
    for &g in gaps.iter() {
        if pos >= cycle {
            break;
        }
        mask |= 1u128 << pos;
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

fn midi_note_num(n: MidiNote) -> u8 {
    u7::from(n).as_int()
}

/// Absolute 1V/oct CV for a MIDI note (0V ≈ MIDI 0 / C-1).
fn midi_note_to_counts(n: MidiNote) -> u16 {
    semis_to_counts(u16::from(midi_note_num(n)).min(120))
}

/// Absolute 1V/oct CV for base MIDI note + cent offset (φ mode).
fn midi_cents_to_counts(base: MidiNote, cents_offset: u32) -> u16 {
    let abs = u32::from(midi_note_num(base)) * 100 + cents_offset;
    cents_to_counts(abs.min(12_000))
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

fn div_for_speed(speed: u8) -> u32 {
    SPEED_DIVS[(speed.min(SPEED_COUNT - 1)) as usize]
}

/// Map a fader value to a speed index. Top of travel = fastest (Heat Pump order).
fn speed_from_fader(value: u16) -> u8 {
    ((value as u32 * SPEED_COUNT as u32) / 4096).min(SPEED_COUNT as u32 - 1) as u8
}

/// Fader latch target: center of the zone for `speed`.
fn fader_for_speed(speed: u8) -> u16 {
    let i = speed.min(SPEED_COUNT - 1) as u32;
    ((i * 2 + 1) * 4096 / (SPEED_COUNT as u32 * 2)) as u16
}

/// Top LED while editing speed: orange = triplet, yellow = dotted, cyan = straight.
fn speed_led_color(speed: u8) -> Color {
    match div_for_speed(speed) {
        8 | 16 => Color::Orange,  // 1/8T, 1/4T
        18 | 36 => Color::Yellow, // dotted
        _ => Color::Cyan,
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

#[embassy_executor::task(pool_size = 4)]
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
            speed: SPEED_DEFAULT as usize,
            color: Color::Violet,
            cv_jack: CV_JACK_OUT,
            range: Range::_0_10V,
            cv_dest: DEST_DEPTH,
            cv_att: 100,
            mode: MODE_GATE_NOTE as usize,
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

    let (midi_out, midi_chan, note, cc, gatel, param_speed, led_color, cv_jack, range, cv_dest, cv_att, param_mode) =
        params.query(|p| {
            (
                p.midi_out,
                p.midi_channel,
                p.note,
                p.cc,
                p.gatel as u32,
                p.speed,
                p.color,
                p.cv_jack.min(1),
                p.range,
                p.cv_dest.min(DEST_COUNT - 1),
                att_from_pct(p.cv_att),
                p.mode.min(3) as u8,
            )
        });

    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let glob_muted = app.make_global(false);
    let glob_reversed = app.make_global(false);
    let glob_cycle = app.make_global(16_u32);
    let glob_depth = app.make_global(5_u32);
    let glob_mask = app.make_global(0_u128);
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

    let (fader_saved, shift_fader_saved, muted, reversed, _stored_mode, speed_saved) =
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

    let glob_fader_raw = app.make_global(fader_saved);
    let glob_cycle_raw = app.make_global(shift_fader_saved);
    let glob_cv_val = app.make_global(2047u16);

    let glob_ticks = app.make_global(0u64);
    let glob_clock_reset = app.make_global(false);
    let glob_clock_stop = app.make_global(false);

    // Mode param is source of truth on spawn (configurator / editor / Shift+long
    // all write it). Keep scene storage in sync so LoadScene stays coherent.
    let out_mode = param_mode;
    storage.modify_and_save(|s| s.out_mode = out_mode);

    // One jack, one role: Note/CC → gate GPO; Pitch/Phi → pitch DAC; In → CV mod.
    // Do not configure DAC then GPO on the same port (that left Note mode silent
    // when the final GPO bring-up raced the DAC setup on some boots).
    // When Jack=CV In, skip outs entirely (MIDI-first).
    let (cv_out, gate_out) = if cv_jack == CV_JACK_OUT {
        if is_pitch_mode(out_mode) {
            let j = app.make_out_jack(0, range).await;
            j.set_value(0);
            (Some(j), None)
        } else {
            let g = app.make_gate_jack(0, 4095).await;
            // make_gate_jack drives the port high on configure; force known-off
            // (also clears any note left sounding by a prior run() dropped
            // mid-gate, e.g. on a param change respawn).
            g.set_low().await;
            (None, Some(g))
        }
    } else {
        (None, None)
    };
    let cv_in = if cv_jack == CV_JACK_IN {
        Some(app.make_in_jack(0, Range::_Neg5_5V).await)
    } else {
        None
    };

    midi.send_note_off(note).await;
    if is_pitch_mode(out_mode) {
        midi.send_pitch_bend(8192).await;
    }

    glob_muted.set(muted);
    glob_reversed.set(reversed);
    glob_mode.set(out_mode);
    glob_speed.set(if speed_saved < SPEED_COUNT {
        speed_saved
    } else {
        (param_speed as u8).min(SPEED_COUNT - 1)
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
        let mut note_on: Option<MidiNote> = None;
        let mut cc_on = false;
        let mut cached_mode = glob_mode.get();
        let mut step = 0u32;
        let mut cached_speed = glob_speed.get();
        let mut div = div_for_speed(cached_speed);
        let mut gate_step = (div * gatel / 100).clamp(1, div - 1);
        let mut last_processed_tick: u64 = 0;

        loop {
            app.delay_millis(1).await;

            if glob_clock_reset.get() || glob_clock_stop.get() {
                glob_clock_reset.set(false);
                glob_clock_stop.set(false);
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
                    if let Some(ref gate_jack) = gate_out { gate_jack.set_low().await; }
                }
                leds.unset(0, Led::Top);
                leds.unset(0, Led::Bottom);
            }

            let tick = glob_ticks.get();
            if tick == last_processed_tick {
                continue;
            }
            last_processed_tick = tick;
            let clkn = tick as u32;

            // Mode changed on the device: reconfigure the jack.
            // Param save respawns run() shortly after; this covers the
            // window until then (and keeps hardware in the right mode).
            let mode = glob_mode.get();
            if mode != cached_mode {
                if cv_jack == CV_JACK_OUT {
                    if is_pitch_mode(mode) {
                        app.make_out_jack(0, range).await;
                        if let Some(ref j) = cv_out {
                            j.set_value(0);
                        }
                    } else {
                        app.make_gate_jack(0, 4095).await;
                        if let Some(ref gate_jack) = gate_out {
                            gate_jack.set_low().await;
                        }
                    }
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

                let hit = glob_mask.get() & (1u128 << step) != 0;
                if hit && !glob_muted.get() {
                    match cached_mode {
                        MODE_GATE_CC => {
                            if let Some(ref gate_jack) = gate_out { gate_jack.set_high().await; }
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
                            let n = { note }.transpose(semis as i8);
                            if let Some(ref cv_jack) = cv_out {
                                cv_jack.set_value(midi_note_to_counts(n));
                            }
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
                            let (n, bend) = note_and_bend(note, cents);
                            if let Some(ref cv_jack) = cv_out {
                                cv_jack.set_value(midi_cents_to_counts(note, cents));
                            }
                            midi.send_pitch_bend(bend).await;
                            midi.send_note_on(n, 4095).await;
                            note_on = Some(n);
                        }
                        _ => {
                            if let Some(ref gate_jack) = gate_out { gate_jack.set_high().await; }
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
                    if let Some(ref gate_jack) = gate_out { gate_jack.set_low().await; }
                }
                leds.set(0, Led::Bottom, led_color, Brightness::Off);
            }

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
                    let cyc = glob_cycle.get();
                    let span = (MAX_CYCLE - MIN_CYCLE).max(1);
                    let norm = ((cyc.saturating_sub(MIN_CYCLE)) * 255 / span) as u8;
                    leds.set(0, Led::Top, Color::Red, Brightness::Custom(norm));
                }
                LatchLayer::Third => {
                    leds.set(
                        0,
                        Led::Top,
                        speed_led_color(glob_speed.get()),
                        LED_BRIGHTNESS,
                    );
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
                            if let Some(ref gate_jack) = gate_out { gate_jack.set_low().await; }
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
                // Shift + long press: cycle output mode (Note / CC / Pitch / Phi).
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
                LatchLayer::Third => fader_for_speed(glob_speed.get()),
            };

            if let Some(new_value) = latch.update(faders.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_fader_raw.set(new_value);
                        glob_depth.set(depth_from_value(new_value));
                        glob_mask.set(build_mask(
                            glob_cycle.get(),
                            glob_depth.get(),
                            glob_reversed.get(),
                        ));
                        storage.modify_and_save(|s| s.fader_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        glob_cycle_raw.set(new_value);
                        glob_cycle.set(cycle_from_value(new_value));
                        glob_mask.set(build_mask(
                            glob_cycle.get(),
                            glob_depth.get(),
                            glob_reversed.get(),
                        ));
                        storage.modify_and_save(|s| s.shift_fader_saved = new_value);
                    }
                    LatchLayer::Third => {
                        // Button held + fader: pick speed (up = fast, Heat Pump grid).
                        glob_fader_moved.set(true);
                        let speed = speed_from_fader(new_value);
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
                    if speed_saved < SPEED_COUNT {
                        glob_speed.set(speed_saved);
                    } else {
                        glob_speed.set((param_speed as u8).min(SPEED_COUNT - 1));
                    }
                    glob_fader_raw.set(fader_saved);
                    glob_cycle_raw.set(shift_fader_saved);
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
        let mut prev_gate_high = false;
        let mut last_depth = glob_depth.get();
        let mut last_cycle = glob_cycle.get();
        loop {
            app.delay_millis(1).await;
            if let Some(ref input) = cv_in {
                let in_val = attenuate_bipolar(input.get_value(), cv_att);
                glob_cv_val.set(in_val);
                match cv_dest {
                    DEST_DEPTH => {
                        let d = depth_from_value(mod_u16(glob_fader_raw.get(), in_val));
                        if d != last_depth {
                            last_depth = d;
                            glob_depth.set(d);
                            glob_mask.set(build_mask(
                                glob_cycle.get(),
                                d,
                                glob_reversed.get(),
                            ));
                        }
                        prev_gate_high = false;
                    }
                    DEST_CYCLE => {
                        let c = cycle_from_value(mod_u16(glob_cycle_raw.get(), in_val));
                        if c != last_cycle {
                            last_cycle = c;
                            glob_cycle.set(c);
                            glob_mask.set(build_mask(
                                c,
                                glob_depth.get(),
                                glob_reversed.get(),
                            ));
                        }
                        prev_gate_high = false;
                    }
                    DEST_RESET => {
                        let high = in_val >= TRIG_HIGH;
                        if high && !prev_gate_high {
                            glob_reset.set(true);
                        }
                        prev_gate_high = high;
                    }
                    _ => {
                        prev_gate_high = false;
                    }
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
        join(clock_drain, join5(clock_processor, fut_buttons, fut_faders, scene_handler, shift)),
    )
    .await;
}
