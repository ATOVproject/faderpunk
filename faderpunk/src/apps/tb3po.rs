// TB-3PO: TB-303 style acid pattern generator for Faderpunk
// Port of the TB-3PO Hemisphere applet by Logarhythm/djphazer
// Copyright (c) 2020, Logarhythm (original C++ implementation, MIT licensed)

use embassy_futures::{
    join::{join, join3, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Instant;
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config,
    MidiChannel, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Global, Led, ManagedStorage, ParamStore, SceneEvent,
};
use crate::tasks::leds::LedMode;

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 3;

const MAX_STEPS: usize = 32;

// 0–10V range: 10V / 120 semitones ≈ 34 counts/semitone (1V/oct standard)
const SEMITONE_COUNTS: i32 = 34;
// Center pitch hint for quantizer input (midpoint of 0–4095 ≈ C5 at 0V=C0)
const CENTER_CV: u16 = 2048;
// One octave in counts
const OCTAVE_COUNTS: i32 = 410;

pub static CONFIG: Config<PARAMS> = Config::new(
    "TB-3PO",
    "TB-303 acid pattern generator",
    Color::Orange,
    AppIcon::SoftRandom,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiOut)
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Orange,
        Color::Green,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
    ],
});

pub struct Params {
    midi_channel: MidiChannel,
    midi_out: MidiOut,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut::default(),
            color: Color::Orange,
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
            midi_out: MidiOut::from_value(values[1]),
            color: Color::from_value(values[2]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seed: u16,
    density_fader: u16,   // 0–4095 → density 0–14
    length_fader: u16,    // 0–4095 → num_steps 1–32
    transpose_fader: u16, // 0–4095 → transpose −24..+24 semitones
    res_saved: u16,       // 0–4095 → index into RESOLUTION table (8 segments of 512)
    lock_seed: bool,
    no_accents: bool,
    no_slides: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seed: 0xABCD,
            density_fader: 2048,   // density 7 (center)
            length_fader: 1920,    // ~16 steps
            transpose_fader: 2048, // 0 semitones
            res_saved: 2048,       // index 4 → RESOLUTION[4] = 6 (16th notes)
            lock_seed: false,
            no_accents: false,
            no_slides: false,
        }
    }
}

impl AppStorage for Storage {}

// --- Acid Pattern Data ---

#[derive(Copy, Clone)]
struct AcidPattern {
    gates: u32,
    slides: u32,
    accents: u32,
    oct_ups: u32,
    oct_downs: u32,
    notes: [u8; MAX_STEPS], // pitch index (scale degree 0–8)
}

impl Default for AcidPattern {
    fn default() -> Self {
        Self {
            gates: 0,
            slides: 0,
            accents: 0,
            oct_ups: 0,
            oct_downs: 0,
            notes: [0; MAX_STEPS],
        }
    }
}

// --- Deterministic PRNG (Xorshift32) ---

fn xorshift32(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

fn rand_below(state: &mut u32, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    xorshift32(state) % max
}

fn rand_bit(state: &mut u32, prob_pct: u32) -> bool {
    rand_below(state, 100) < prob_pct
}

// --- Pattern Generation ---

fn generate_pattern(seed: u16, density: u8) -> AcidPattern {
    let density = density.min(14);
    let on_off_dens = (density as i8 - 7).unsigned_abs(); // 0–7; 7 = most gates
    let pitch_change_dens = density.min(8); // 0–8 pitch variety

    // Phase 1: Pitch content (seeded with seed+1)
    let mut rng: u32 = (seed as u32).wrapping_add(1).max(1);

    let available_pitches: u32 = match pitch_change_dens {
        0 => 0,
        1 => 1,
        d => d as u32 - 1,
    };

    let mut notes = [0u8; MAX_STEPS];
    let mut oct_ups: u32 = 0;
    let mut oct_downs: u32 = 0;

    for s in 0..MAX_STEPS {
        let repeat_prob = 50u32.saturating_sub(pitch_change_dens as u32 * 6);
        if s > 0 && rand_bit(&mut rng, repeat_prob) {
            // Repeat previous note; oct shift bits are NOT updated on repeats (matches original)
            notes[s] = notes[s - 1];
        } else {
            notes[s] = rand_below(&mut rng, available_pitches + 1) as u8;
            // Octave shift: 40% chance of up or down (accumulated by left-shift, matches original)
            oct_ups <<= 1;
            oct_downs <<= 1;
            let coinflip = rand_below(&mut rng, 200);
            if coinflip < 80 {
                if coinflip & 1 == 1 {
                    oct_ups |= 1;
                } else {
                    oct_downs |= 1;
                }
            }
        }
    }

    // Phase 2: Gates / slides / accents (seeded with seed+2)
    let mut rng: u32 = (seed as u32).wrapping_add(2).max(1);

    // At on_off_dens=7: dens_prob=108 (always gate). At 0: dens_prob=10 (very sparse)
    let dens_prob = 10 + on_off_dens as u32 * 14;

    let mut gates: u32 = 0;
    let mut slides: u32 = 0;
    let mut accents: u32 = 0;
    let mut latest_slide = false;
    let mut latest_accent = false;

    for _ in 0..MAX_STEPS {
        // All bit-fields accumulated left-to-right; step N lives at bit (31−N) after 32 iters,
        // but is read back at bit N via step_is_* — this matches the original's behaviour.
        gates = (gates << 1) | rand_bit(&mut rng, dens_prob) as u32;

        let new_slide = rand_bit(&mut rng, if latest_slide { 10 } else { 18 });
        slides = (slides << 1) | new_slide as u32;
        latest_slide = new_slide;

        let new_accent = rand_bit(&mut rng, if latest_accent { 7 } else { 16 });
        accents = (accents << 1) | new_accent as u32;
        latest_accent = new_accent;
    }

    AcidPattern {
        gates,
        slides,
        accents,
        oct_ups,
        oct_downs,
        notes,
    }
}

fn step_is_gated(p: &AcidPattern, step: u8) -> bool {
    (p.gates >> step) & 1 != 0
}

fn step_is_slid(p: &AcidPattern, step: u8) -> bool {
    (p.slides >> step) & 1 != 0
}

fn step_is_accent(p: &AcidPattern, step: u8) -> bool {
    (p.accents >> step) & 1 != 0
}

fn step_is_oct_up(p: &AcidPattern, step: u8) -> bool {
    (p.oct_ups >> step) & 1 != 0
}

fn step_is_oct_down(p: &AcidPattern, step: u8) -> bool {
    (p.oct_downs >> step) & 1 != 0
}

/// Raw pitch CV for a step before quantising (in ±5V counts 0–4095).
fn raw_pitch_cv(p: &AcidPattern, step: u8, transpose: i16) -> u16 {
    let note = p.notes[step as usize] as i32;
    let cv = CENTER_CV as i32
        + note * SEMITONE_COUNTS
        + if step_is_oct_up(p, step) {
            OCTAVE_COUNTS
        } else if step_is_oct_down(p, step) {
            -OCTAVE_COUNTS
        } else {
            0
        }
        + transpose as i32 * SEMITONE_COUNTS;
    cv.clamp(0, 4095) as u16
}

// --- Glide (copied from midi2cv.rs; will move to libfp in a future PR) ---

/// Apply RC-filter exponential glide: moves current toward target by coeff each tick.
fn apply_glide(current: f32, target: f32, coeff: f32) -> f32 {
    current + (target - current) * coeff
}

/// Fixed 303-style slide coefficient: 1 - e^(-1/21) ≈ 0.0465 → ~100 ms time constant.
/// Derived from calc_glide_coeff(40) in midi2cv.rs (glide=40 → tau=21 ms).
const SLIDE_COEFF: f32 = 0.0465_f32;

// --- Embassy Task ---

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
    let pitch_range = Range::_0_10V;
    let accent_range = Range::_0_5V;

    let (midi_out, midi_chan, led_color) = params.query(|p| (p.midi_out, p.midi_channel, p.color));

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let quantizer = app.use_quantizer(pitch_range);
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let pitch_out = app.make_out_jack(0, pitch_range).await;
    let gate_out = app.make_gate_jack(1, 4095).await;
    let accent_out = app.make_out_jack(2, accent_range).await;

    // Clock resolution table: 24-ppqn divisors ordered slow → fast.
    // Fader 0 is split into 8 equal segments (0–511, 512–1023, …), each selecting one entry.
    // Default res_saved=2048 → index 4 → div=6 (16th notes, matching original ClockDivision::_6).
    //   idx:  0    1   2   3   4  5  6  7
    //   div: [96,  48, 24, 12,  6, 4, 3, 2]
    //   note: 1    ½   ¼   8th 16th 16t 32nd fast
    let resolution: [usize; 8] = [96, 48, 24, 12, 6, 4, 3, 2];

    // --- Runtime-only globals (not mirrored in storage) ---
    let step_glob: Global<u8> = app.make_global(0);
    let pattern_glob: Global<AcidPattern> = app.make_global(AcidPattern::default());
    // slide_target holds quantised CV counts; output_task interpolates toward it
    let slide_target_glob: Global<u16> = app.make_global(CENTER_CV);
    let slide_active_glob: Global<bool> = app.make_global(false);
    let gate_off_ms_glob: Global<u32> = app.make_global(0);
    let gate_active_glob: Global<bool> = app.make_global(false);
    let accent_active_glob: Global<bool> = app.make_global(false);
    let last_midi_note_glob: Global<MidiNote> = app.make_global(MidiNote::default());
    // Signals fader_task → clock_task that density changed and pattern needs regenerating
    let regen_pending_glob: Global<bool> = app.make_global(false);
    // Current clock divisor (raw 24-PPQN units); updated by fader_task via resolution table
    let (init_res, init_density_fader, init_seed) =
        storage.query(|s| (s.res_saved, s.density_fader, s.seed));
    let div_glob: Global<usize> = app.make_global(resolution[init_res as usize / 512]);
    // Active latch layer for fader 0: Main = density, Third = clock resolution
    let latch_layer_glob: Global<LatchLayer> = app.make_global(LatchLayer::Main);

    // --- Initialise pattern from storage ---
    let init_density = (init_density_fader as u32 * 14 / 4095) as u8;
    pattern_glob.set(generate_pattern(init_seed, init_density));
    leds.set(0, Led::Button, led_color, Brightness::Low);

    // Fader latches for smooth takeover
    let mut latches: [libfp::latch::AnalogLatch; CHANNELS] =
        core::array::from_fn(|i| app.make_latch(faders.get_value_at(i)));

    // --- Clock task: step advance, quantise pitch, fire gate ---
    let clock_task = async {
        let mut last_tick = Instant::now();
        let mut cycle_ms: u32 = 500;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick => {
                    let clkn = ticks() as usize;
                    let div = div_glob.get();
                    let in_res_mode = latch_layer_glob.get() == LatchLayer::Third;

                    // Division LED: flash on at step boundary, off at half-cycle.
                    // Orange = straight (power-of-2 divisors), Blue = triplet.
                    if in_res_mode {
                        if clkn.is_multiple_of(div) {
                            let color = if matches!(div, 2 | 4 | 8 | 16) {
                                Color::Orange
                            } else {
                                Color::Blue
                            };
                            leds.set(0, Led::Bottom, color, Brightness::High);
                        } else if clkn % div == (div / 2).max(1) {
                            leds.unset(0, Led::Bottom);
                        }
                    }

                    if !clkn.is_multiple_of(div) {
                        continue;
                    }

                    // Measure clock period
                    let now = Instant::now();
                    let delta = now.duration_since(last_tick).as_millis() as u32;
                    if delta > 0 && delta < 5000 {
                        cycle_ms = delta;
                    }
                    last_tick = now;

                    let pattern = pattern_glob.get();
                    let (num_steps, no_accents, no_slides, transpose) = storage.query(|s| {
                        (
                            (s.length_fader as u32 * 31 / 4095 + 1) as u8,
                            s.no_accents,
                            s.no_slides,
                            (s.transpose_fader as i32 * 48 / 4095 - 24) as i16,
                        )
                    });

                    // Derive step from absolute tick counter — phase-locked to clock.
                    // clkn == 0 means first tick after a Reset; treat as no previous step.
                    let step = (clkn / div % num_steps as usize) as u8;
                    let prev_step = if clkn == 0 {
                        None
                    } else {
                        Some((step as usize + num_steps as usize - 1) % num_steps as usize)
                    };
                    step_glob.set(step);

                    let is_gated = step_is_gated(&pattern, step);
                    let is_slid_prev = !no_slides
                        && prev_step
                            .map(|p| step_is_slid(&pattern, p as u8))
                            .unwrap_or(false);
                    let is_accent = !no_accents && step_is_accent(&pattern, step);
                    let target_raw = raw_pitch_cv(&pattern, step, transpose);

                    // Pitch / slide
                    if is_slid_prev {
                        // Glide: output_task will interpolate toward new target
                        let out = quantizer.get_quantized_note(target_raw).await;
                        slide_target_glob.set(out.as_counts(pitch_range));
                        slide_active_glob.set(true);
                    } else if is_gated {
                        // Snap to new pitch
                        let out = quantizer.get_quantized_note(target_raw).await;
                        let counts = out.as_counts(pitch_range);
                        slide_target_glob.set(counts);
                        slide_active_glob.set(false);
                    }

                    // Gate / MIDI
                    if is_gated || is_slid_prev {
                        accent_active_glob.set(is_accent);

                        // Quantise for MIDI note (use target pitch for note identity)
                        let out = quantizer.get_quantized_note(target_raw).await;
                        let note = out.as_midi();

                        midi.send_note_off(last_midi_note_glob.get()).await;
                        let velocity = if is_accent { 4095 } else { 2048 };
                        midi.send_note_on(note, velocity).await;
                        last_midi_note_glob.set(note);

                        gate_out.set_high().await;
                        gate_active_glob.set(true);
                        accent_out.set_value(if is_accent { 4095 } else { 0 });
                        gate_off_ms_glob.set(cycle_ms / 2);
                    }

                    // Apply any pending pattern regeneration (density changed since last tick)
                    if regen_pending_glob.get() {
                        let (s, df) = storage.query(|s| (s.seed, s.density_fader));
                        let d = (df as u32 * 14 / 4095) as u8;
                        pattern_glob.set(generate_pattern(s, d));
                        regen_pending_glob.set(false);
                    }
                }

                ClockEvent::Reset => {
                    if !storage.query(|s| s.lock_seed) {
                        let new_seed = (ticks() & 0xFFFF) as u16;
                        storage.modify_and_save(|s| s.seed = new_seed);
                        let d = (storage.query(|s| s.density_fader) as u32 * 14 / 4095) as u8;
                        pattern_glob.set(generate_pattern(new_seed, d));
                    } else {
                        let (s, df) = storage.query(|s| (s.seed, s.density_fader));
                        let d = (df as u32 * 14 / 4095) as u8;
                        pattern_glob.set(generate_pattern(s, d));
                    }
                    step_glob.set(0);
                    slide_active_glob.set(false);
                    gate_off_ms_glob.set(0);
                    gate_active_glob.set(false);
                    gate_out.set_low().await;
                    midi.send_note_off(last_midi_note_glob.get()).await;
                    accent_out.set_value(0);
                }

                ClockEvent::Stop => {
                    gate_off_ms_glob.set(0);
                    gate_active_glob.set(false);
                    gate_out.set_low().await;
                    midi.send_note_off(last_midi_note_glob.get()).await;
                    accent_out.set_value(0);
                }

                _ => {}
            }
        }
    };

    // --- Output task: pitch slide + gate-off timing ---
    let output_task = async {
        // Slide tracks quantised counts directly — no re-quantisation during glide
        // (matches original TB3PO which outputs the sliding raw CV without re-snapping)
        let mut glide_current: f32 = CENTER_CV as f32;

        loop {
            app.delay_millis(1).await;

            // Gate-off countdown
            let ms = gate_off_ms_glob.get();
            if ms > 0 {
                let new_ms = ms - 1;
                gate_off_ms_glob.set(new_ms);
                if new_ms == 0 {
                    // Keep gate on during a slide (like the 303)
                    let pattern = pattern_glob.get();
                    let step = step_glob.get();
                    if storage.query(|s| s.no_slides) || !step_is_slid(&pattern, step) {
                        gate_active_glob.set(false);
                        gate_out.set_low().await;
                        midi.send_note_off(last_midi_note_glob.get()).await;
                        accent_out.set_value(0);
                    }
                }
            }

            // Pitch slide interpolation
            let target = slide_target_glob.get() as f32;
            if slide_active_glob.get() {
                glide_current = apply_glide(glide_current, target, SLIDE_COEFF);
                if (glide_current - target).abs() < 0.5 {
                    glide_current = target;
                    slide_active_glob.set(false);
                }
            } else {
                glide_current = target;
            }

            pitch_out.set_value(glide_current as u16);
        }
    };

    // --- Latch-layer polling task: B0 held → fader 0 controls resolution ---
    let layer_task = async {
        let mut prev_in_res = false;
        loop {
            app.delay_millis(1).await;
            let in_res = buttons.is_button_pressed(0) && !buttons.is_shift_pressed();
            // When leaving res mode, clear the division flash LED
            if prev_in_res && !in_res {
                leds.unset(0, Led::Bottom);
            }
            prev_in_res = in_res;
            latch_layer_glob.set(if in_res {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            });
        }
    };

    // --- Fader task ---
    let fader_task = async {
        loop {
            let chan = faders.wait_for_any_change().await;

            // Fader 0: Main = density, Third (B0 held) = clock resolution
            // Faders 1, 2: always Main
            let latch_layer = if chan == 0 {
                latch_layer_glob.get()
            } else {
                LatchLayer::Main
            };

            let target_value = match (chan, latch_layer) {
                (0, LatchLayer::Third) => storage.query(|s| s.res_saved),
                (0, _) => storage.query(|s| s.density_fader),
                (1, _) => storage.query(|s| s.length_fader),
                (2, _) => storage.query(|s| s.transpose_fader),
                _ => continue,
            };

            if let Some(val) =
                latches[chan].update(faders.get_value_at(chan), latch_layer, target_value)
            {
                match (chan, latch_layer) {
                    (0, LatchLayer::Third) => {
                        div_glob.set(resolution[val as usize / 512]);
                        storage.modify_and_save(|s| s.res_saved = val);
                    }
                    (0, _) => {
                        storage.modify_and_save(|s| s.density_fader = val);
                        regen_pending_glob.set(true);
                    }
                    (1, _) => {
                        storage.modify_and_save(|s| s.length_fader = val);
                    }
                    (2, _) => {
                        storage.modify_and_save(|s| s.transpose_fader = val);
                    }
                    _ => {}
                }
            }
        }
    };

    // --- Button short-press task ---
    let button_task = async {
        loop {
            let (chan, is_shift) = buttons.wait_for_any_down().await;
            if is_shift {
                continue;
            }
            match chan {
                0 => {
                    // Reseed on short tap — but only when fader 0 is not in resolution mode
                    if latch_layer_glob.get() != LatchLayer::Third
                        && !storage.query(|s| s.lock_seed)
                    {
                        let new_seed = (ticks() & 0xFFFF) as u16;
                        storage.modify_and_save(|s| s.seed = new_seed);
                        let d = (storage.query(|s| s.density_fader) as u32 * 14 / 4095) as u8;
                        pattern_glob.set(generate_pattern(new_seed, d));
                        step_glob.set(0);
                        leds.set_mode(
                            0,
                            Led::Button,
                            LedMode::FlashThenStatic(Color::White, 1, led_color, Brightness::Low),
                        );
                    }
                }
                1 => {
                    let v = !storage.query(|s| s.no_slides);
                    storage.modify_and_save(|s| s.no_slides = v);
                }
                2 => {
                    let v = !storage.query(|s| s.no_accents);
                    storage.modify_and_save(|s| s.no_accents = v);
                }
                _ => {}
            }
        }
    };

    // --- Button long-press task: B0 toggles lock_seed ---
    let button_long_task = async {
        loop {
            let (chan, _) = buttons.wait_for_any_long_press().await;
            if chan == 0 {
                let v = !storage.query(|s| s.lock_seed);
                storage.modify_and_save(|s| s.lock_seed = v);
            }
        }
    };

    // --- LED task (16 ms) ---
    let led_task = async {
        loop {
            app.delay_millis(16).await;

            let (density_fader, locked, no_accents, no_slides, length_fader, res_saved, transpose_fader) =
                storage.query(|s| {
                    (
                        s.density_fader,
                        s.lock_seed,
                        s.no_accents,
                        s.no_slides,
                        s.length_fader,
                        s.res_saved,
                        s.transpose_fader,
                    )
                });
            let density = (density_fader as u32 * 14 / 4095) as u8;
            let num_steps = (length_fader as u32 * 31 / 4095 + 1) as u8;
            let gate_active = gate_active_glob.get();
            let accent = accent_active_glob.get();
            let slide_active = slide_active_glob.get();
            let step = step_glob.get();
            let in_res_mode = latch_layer_glob.get() == LatchLayer::Third;

            // Ch 0: density (normal) or resolution (while B0 held).
            // In res mode, Bottom LED is driven by clock_task (division flash) — don't touch it here.
            if in_res_mode {
                // Show resolution index as brightness on Top (0–7 → dim to bright)
                let res_idx = (res_saved as usize / 512).min(7) as u8;
                leds.set(0, Led::Top, Color::Cyan, Brightness::Custom(res_idx * 32 + 16));
            } else {
                leds.set(
                    0,
                    Led::Top,
                    led_color,
                    Brightness::Custom((density as u32 * 255 / 14) as u8),
                );
                if locked {
                    leds.set(0, Led::Bottom, Color::Orange, Brightness::High);
                } else {
                    leds.unset(0, Led::Bottom);
                }
            }
            // Button LED managed by reseed flash — only set here on first frame (handled by init)

            // Ch 1: step progress (bright at step 0, dims toward end so loop reset is visible)
            let progress = if num_steps > 0 {
                (255u32.saturating_sub(step as u32 * 255 / num_steps as u32)) as u8
            } else {
                255
            };
            leds.set(1, Led::Top, led_color, Brightness::Custom(progress));
            if slide_active {
                leds.set(1, Led::Bottom, led_color, Brightness::High);
            } else {
                leds.unset(1, Led::Bottom);
            }
            leds.set(
                1,
                Led::Button,
                led_color,
                if no_slides { Brightness::Low } else { Brightness::Mid },
            );

            // Ch 2: gate on Top (primary), accent on Bottom (modifier)
            // When gate is idle, Top shows transpose position (center=dim, extremes=bright)
            if gate_active {
                leds.set(2, Led::Top, led_color, Brightness::High);
            } else {
                let dist = (transpose_fader as i32 - 2048).unsigned_abs() as u32;
                let b = (dist * 255 / 2048) as u8;
                leds.set(2, Led::Top, led_color, Brightness::Custom(b));
            }
            if accent && gate_active {
                leds.set(2, Led::Bottom, Color::Orange, Brightness::High);
            } else {
                leds.unset(2, Led::Bottom);
            }
            leds.set(
                2,
                Led::Button,
                led_color,
                if no_accents { Brightness::Low } else { Brightness::Mid },
            );
        }
    };

    // --- Scene task ---
    let scene_task = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (s, df, res) = storage.query(|s| (s.seed, s.density_fader, s.res_saved));
                    let d = (df as u32 * 14 / 4095) as u8;
                    pattern_glob.set(generate_pattern(s, d));
                    div_glob.set(resolution[res as usize / 512]);
                    step_glob.set(0);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join(
        join5(clock_task, output_task, fader_task, layer_task, button_task),
        join3(button_long_task, led_task, scene_task),
    )
    .await;
}
