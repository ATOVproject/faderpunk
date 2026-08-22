use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{Duration, Instant};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    quantizer::Pitch,
    utils::{attenuate, attenuverter, split_unsigned_value},
    AppIcon, Brightness, ClockDivision, Color, Config, MidiCc, MidiChannel, MidiMode, MidiNote,
    MidiOut, Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 11;

/// Mid→Low button duck duration for CC samples (Note uses gate hold).
const BUTTON_DUCK_MS: u16 = 25;

/// Samples within this 12-bit distance count as "unchanged" (ADC noise floor).
const SAMPLE_DEADBAND: u16 = 24;
/// After this many consecutive unchanged division ticks the input is
/// considered idle (unpatched/static) and no new notes/CCs are emitted.
const IDLE_TICKS: u8 = 2;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Hold Sam",
    "Clocked sample & hold to MIDI note/CC",
    Color::Cyan,
    AppIcon::NoteBox,
)
.add_param(Param::MidiMode)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote {
    name: "Base Note",
})
.add_param(Param::MidiCc { name: "CC number" })
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
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::MidiOut)
.add_param(Param::VoltPerOct)
.add_param(Param::bool { name: "Bypass quantizer" })
.add_param(Param::i32 {
    name: "Attenuverter",
    min: 0,
    max: 4095,
})
.add_param(Param::i32 {
    name: "Base Velocity",
    min: 1,
    max: 127,
});

pub struct Params {
    midi_mode: MidiMode,
    midi_channel: MidiChannel,
    midi_note: MidiNote,
    midi_cc: MidiCc,
    midi_out: MidiOut,
    color: Color,
    range: Range,
    vpo: VoltPerOct,
    bypass: bool,
    att: i32,
    base_velocity: i32,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_mode: MidiMode::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_note: MidiNote::from_value(values[2]),
            midi_cc: MidiCc::from_value(values[3]),
            color: Color::from_value(values[4]),
            range: Range::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
            vpo: VoltPerOct::from_value(values[7]),
            bypass: bool::from_value(values[8]),
            att: i32::from_value(values[9]),
            base_velocity: i32::from_value(values[10]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        vec.push(self.bypass.into()).unwrap();
        vec.push(self.att.into()).unwrap();
        vec.push(self.base_velocity.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Main fader: gate % (Note) or CC amount (CC), 0–4095
    gate_saved: u16,
    /// Alt fader: clock division, 0–4095
    res_saved: u16,
    /// Third fader: velocity sensitivity, 0–4095
    sens_saved: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            // ~90% of division — S&H hold, not a short beep
            gate_saved: 3686,
            res_saved: 2048,
            sens_saved: 2048,
            muted: false,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_mode: MidiMode::default(),
            midi_channel: MidiChannel::default(),
            midi_note: MidiNote::from(36),
            midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
            midi_out: MidiOut::default(),
            color: Color::Cyan,
            range: Range::_0_10V,
            vpo: VoltPerOct::Standard,
            bypass: false,
            att: 4095,
            base_velocity: 100,
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

fn gate_pct_from_fader(value: u16) -> u32 {
    ((value as u32 * 99) / 4095 + 1).clamp(1, 100)
}

fn velocity_12bit(base: i32, sens: u16, delta: u16) -> u16 {
    let vel_7 = (base + (sens as i32 * delta as i32) / 4095).clamp(1, 127) as u32;
    ((vel_7 * 4095) / 127) as u16
}

/// Gate length in PPQN ticks as % of the current division (tempo-relative).
/// 100% = full division (held until the next sample cuts it).
fn gate_ticks(div: u32, gate_saved: u16) -> u64 {
    let pct = gate_pct_from_fader(gate_saved);
    let div = div.max(1) as u64;
    (div * pct as u64 / 100).clamp(1, div)
}

/// Base Note at 0V (C0 / MIDI 12); CV adds semitones above that.
fn note_from_cv(base: MidiNote, pitch: Pitch) -> MidiNote {
    const C0_MIDI: i32 = 12;
    let base_i = u7::from(base).as_int() as i32;
    let cv_i = u7::from(pitch.as_midi()).as_int() as i32;
    MidiNote::from((base_i + cv_i - C0_MIDI).clamp(0, 127))
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {

    let (
        midi_out,
        midi_mode,
        base_note,
        midi_cc,
        led_color,
        midi_chan,
        range,
        vpo,
        bypass,
        att,
        base_velocity,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_mode,
            p.midi_note,
            p.midi_cc,
            p.color,
            p.midi_channel,
            p.range,
            p.vpo,
            p.bypass,
            p.att.clamp(0, 4095) as u16,
            p.base_velocity,
        )
    });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let quantizer = app.use_quantizer(range, vpo, bypass);
    let midi = app.use_midi_output(midi_out, midi_chan, false);
    let input = app.make_in_jack(0, range).await;

    // TODO(shared-utils): first 12 entries of libfp::utils::CLOCK_DIVISIONS, and the
    // `res / 345` lookups below are what `division_at(res, 12)` does — with the bounds
    // check built in. Swap both once that helper is on this branch's base.
    let resolution = [384u32, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_clock_tick = app.make_global(0u64);
    let div_glob = app.make_global(4u32);
    let glob_muted = app.make_global(storage.query(|s| s.muted));
    let transport_stopped = app.make_global(false);
    let long_press_fired = app.make_global(false);
    let fader_moved_during_hold = app.make_global(false);

    let midi_note = app.make_global(MidiNote::from(0));
    let note_on = app.make_global(false);
    let prev_sample = app.make_global(0u16);
    // Consecutive division ticks with an unchanged input sample. Starts idle
    // so an unpatched jack stays silent from the very first tick.
    let idle_count = app.make_global(IDLE_TICKS);
    // Tempo-relative note-off (PPQN tick count). u64::MAX = none pending.
    let gate_end_clkn = app.make_global(u64::MAX);
    // Wall-clock fallback so Stop (ticks pause) can still finish the gate.
    let gate_end_instant = app.make_global(Instant::from_ticks(0));
    let last_tick_at = app.make_global(Instant::now());
    // Measured ms between PPQN ticks (for Stop fallback).
    let tick_period_ms = app.make_global(21u64);
    // Mid→Low button duck (CC samples / brief hit); Note stays Low while gated.
    let button_duck = app.make_global(0u16);
    // Brief Top/Button flash on each new sample.
    let sample_flash = app.make_global(0u16);

    let (res, muted) = storage.query(|s| (s.res_saved, s.muted));
    div_glob.set(resolution[(res as usize / 345).min(resolution.len() - 1)]);
    glob_muted.set(muted);

    if muted {
        leds.set(0, Led::Button, Color::Red, Brightness::Low);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let fut_clock = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    // Keep sample and sounding note; only the phase counter resets.
                }
                ClockEvent::Start => {
                    transport_stopped.set(false);
                }
                ClockEvent::Stop => {
                    transport_stopped.set(true);
                    // Gate continues until gate_end_instant (polled by fut_release).
                }
                ClockEvent::Tick(tick) => {
                    glob_clock_tick.set(tick);
                    let now = Instant::now();
                    let dt = now
                        .checked_duration_since(last_tick_at.get())
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    if (1..500).contains(&dt) {
                        tick_period_ms.set(dt);
                    }
                    last_tick_at.set(now);

                    let clkn = tick;
                    let div = div_glob.get().max(1);

                    // Tempo-relative note-off while the clock is running
                    if midi_mode == MidiMode::Note
                        && note_on.get()
                        && !transport_stopped.get()
                        && clkn >= gate_end_clkn.get()
                    {
                        midi.send_note_off(midi_note.get()).await;
                        note_on.set(false);
                        gate_end_clkn.set(u64::MAX);
                        leds.set(0, Led::Top, led_color, Brightness::Off);
                    }

                    if clkn.is_multiple_of(div as u64) {
                        let blocked = glob_muted.get() || transport_stopped.get();
                        let raw = input.get_value();
                        let sample = attenuverter(raw, att);
                        let prev = prev_sample.get();
                        let delta = sample.abs_diff(prev);
                        prev_sample.set(sample);

                        // Idle detection: an unpatched/static jack reads the
                        // same value every tick; stop emitting once the input
                        // has been unchanged for IDLE_TICKS division ticks.
                        if delta > SAMPLE_DEADBAND {
                            idle_count.set(0);
                        } else {
                            idle_count.set(idle_count.get().saturating_add(1));
                        }

                        if !blocked && idle_count.get() < IDLE_TICKS {

                            match midi_mode {
                                MidiMode::Note => {
                                    let pitch = quantizer.get_quantized_note(sample).await;
                                    let note = note_from_cv(base_note, pitch);
                                    let sens = storage.query(|s| s.sens_saved);
                                    let vel = velocity_12bit(base_velocity, sens, delta);

                                    if note_on.get() {
                                        midi.send_note_off(midi_note.get()).await;
                                    }
                                    midi.send_note_on(note, vel).await;
                                    midi_note.set(note);
                                    note_on.set(true);
                                    sample_flash.set(80);

                                    let gate_saved = storage.query(|s| s.gate_saved);
                                    let gt = gate_ticks(div, gate_saved);
                                    gate_end_clkn.set(clkn.saturating_add(gt));
                                    gate_end_instant.set(
                                        Instant::now()
                                            + Duration::from_millis(
                                                gt.saturating_mul(tick_period_ms.get().max(1)),
                                            ),
                                    );
                                }
                                MidiMode::Cc => {
                                    let amount = storage.query(|s| s.gate_saved);
                                    let cc_val = attenuate(sample, amount);
                                    midi.send_cc(midi_cc, cc_val).await;
                                    button_duck.set(BUTTON_DUCK_MS);
                                    sample_flash.set(80);
                                }
                            }
                        }

                        if buttons.is_shift_pressed() {
                            if matches!(div, 2 | 4 | 8 | 16) {
                                leds.set(0, Led::Bottom, Color::Orange, Brightness::High);
                            } else {
                                leds.set(0, Led::Bottom, Color::Blue, Brightness::High);
                            }
                        }
                    }
                }
            }
        }
    };

    let fut_release = async {
        loop {
            app.delay_millis(1).await;
            // Wall-clock path: needed after Stop, or when an external clock
            // simply stops ticking without sending a Stop event.
            let now = Instant::now();
            let clock_stalled = now
                >= last_tick_at.get()
                    + Duration::from_millis(tick_period_ms.get().max(1).saturating_mul(4));
            if midi_mode == MidiMode::Note
                && note_on.get()
                && (transport_stopped.get() || clock_stalled)
                && now >= gate_end_instant.get()
            {
                midi.send_note_off(midi_note.get()).await;
                note_on.set(false);
                gate_end_clkn.set(u64::MAX);
                leds.set(0, Led::Top, led_color, Brightness::Off);
            }
        }
    };

    let fut_fader = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut last_fader = fader.get_value();
        loop {
            fader.wait_for_change().await;
            let value = fader.get_value();

            if buttons.is_button_pressed(0) && !buttons.is_shift_pressed() && value != last_fader {
                fader_moved_during_hold.set(true);
            }
            last_fader = value;

            let latch_layer = glob_latch_layer.get();
            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.gate_saved),
                LatchLayer::Alt => storage.query(|s| s.res_saved),
                LatchLayer::Third => storage.query(|s| s.sens_saved),
            };

            if let Some(new_value) = latch.update(value, latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.gate_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        div_glob.set(
                            resolution[(new_value as usize / 345).min(resolution.len() - 1)],
                        );
                        storage.modify_and_save(|s| s.res_saved = new_value);
                    }
                    LatchLayer::Third => {
                        storage.modify_and_save(|s| s.sens_saved = new_value);
                    }
                }
            }
        }
    };

    let fut_buttons = async {
        loop {
            let shift = buttons.wait_for_down(0).await;
            if shift {
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() && !glob_muted.get() {
                    // Shift+short = manual sample
                    let raw = input.get_value();
                    let sample = attenuverter(raw, att);
                    let prev = prev_sample.get();
                    let delta = sample.abs_diff(prev);
                    prev_sample.set(sample);

                    match midi_mode {
                        MidiMode::Note => {
                            let pitch = quantizer.get_quantized_note(sample).await;
                            let note = note_from_cv(base_note, pitch);
                            let sens = storage.query(|s| s.sens_saved);
                            let vel = velocity_12bit(base_velocity, sens, delta);

                            if note_on.get() {
                                midi.send_note_off(midi_note.get()).await;
                            }
                            midi.send_note_on(note, vel).await;
                            midi_note.set(note);
                            note_on.set(true);
                            sample_flash.set(80);

                            let div = div_glob.get().max(1);
                            let gate_saved = storage.query(|s| s.gate_saved);
                            let gt = gate_ticks(div, gate_saved);
                            let clkn = glob_clock_tick.get();
                            gate_end_clkn.set(clkn.saturating_add(gt));
                            gate_end_instant.set(
                                Instant::now()
                                    + Duration::from_millis(
                                        gt.saturating_mul(tick_period_ms.get().max(1)),
                                    ),
                            );
                        }
                        MidiMode::Cc => {
                            let amount = storage.query(|s| s.gate_saved);
                            let cc_val = attenuate(sample, amount);
                            midi.send_cc(midi_cc, cc_val).await;
                            button_duck.set(BUTTON_DUCK_MS);
                            sample_flash.set(80);
                        }
                    }
                }
            } else {
                fader_moved_during_hold.set(false);
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                // Skip the toggle when the hold was used to adjust the
                // Third-layer fader (velocity sensitivity).
                if !long_press_fired.get() && !fader_moved_during_hold.get() {
                    // Soft mute: ausklingen — no hard note off
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| {
                        s.muted = muted;
                    });
                    if muted {
                        leds.set(0, Led::Button, Color::Red, Brightness::Low);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }
                }
            }
        }
    };

    let fut_layer = async {
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

            let display = match latch_active_layer {
                LatchLayer::Main => storage.query(|s| s.gate_saved),
                LatchLayer::Alt => storage.query(|s| s.res_saved),
                LatchLayer::Third => storage.query(|s| s.sens_saved),
            };

            let flash = sample_flash.get();
            if flash > 0 {
                sample_flash.set(flash.saturating_sub(1));
                leds.set(0, Led::Top, Color::White, Brightness::High);
                leds.set(0, Led::Bottom, spectrum_color(display), Brightness::Mid);
                if !glob_muted.get() {
                    leds.set(0, Led::Button, Color::White, Brightness::High);
                }
            } else if glob_muted.get() {
                leds.set(0, Led::Button, Color::Red, Brightness::Low);
                leds.unset(0, Led::Top);
                leds.unset(0, Led::Bottom);
            } else {
                let color = spectrum_color(display);
                let btn = if note_on.get() || button_duck.get() > 0 {
                    48u8
                } else {
                    (display / 16).max(40) as u8
                };
                paint_fader_meters(&leds, color, display, btn);
            }

            let duck = button_duck.get();
            if duck > 0 {
                button_duck.set(duck.saturating_sub(1));
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (res, muted) = storage.query(|s| (s.res_saved, s.muted));
                    div_glob.set(resolution[(res as usize / 345).min(resolution.len() - 1)]);
                    glob_muted.set(muted);
                    if muted {
                        leds.set(0, Led::Button, Color::Red, Brightness::Low);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let fut_long_press = async {
        loop {
            let (_, shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            if !shift && !fader_moved_during_hold.get() {
                // Hard panic
                if midi_mode == MidiMode::Note && note_on.get() {
                    midi.send_note_off(midi_note.get()).await;
                    note_on.set(false);
                    // Push gate end into the past so release paths are a no-op
                    gate_end_instant.set(Instant::from_ticks(0));
                    gate_end_clkn.set(0);
                }
                if !glob_muted.get() {
                    glob_muted.set(true);
                    storage.modify_and_save(|s| s.muted = true);
                    leds.set(0, Led::Button, Color::Red, Brightness::Low);
                }
            }
        }
    };

    join(
        join5(fut_clock, fut_fader, fut_buttons, fut_layer, scene_handler),
        join(fut_long_press, fut_release),
    )
    .await;
}

fn spectrum_color(fader: u16) -> Color {
    let hue = (fader.min(4095) as u32 * 270 / 4095) as u16;
    let (r, g, b) = hsv_to_rgb(hue, 255, 255);
    Color::Custom(r, g, b)
}

fn paint_fader_meters(leds: &crate::app::Leds<CHANNELS>, color: Color, fader: u16, button_bright: u8) {
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

fn hsv_to_rgb(hue: u16, sat: u8, val: u8) -> (u8, u8, u8) {
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
