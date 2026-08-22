//! Control Issues — shape incoming CV and send it as MIDI CC / NRPN.
//!
//! Pipeline: Input/Gate → Attenuverter → Offset → Transfer → Slew → Steps → Min/Max → Rate-Limit
//!
//! Layers: Main = attenuverter, Alt (Shift) = offset, Third (button+fader) = slew.
//! Button click (no fader move) = mute; Shift+Button = momentary bypass.

use embassy_futures::{
    join::join5,
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{
        attenuverter, midi_gate, scale_bits_7_12, split_signed_value, split_unsigned_value,
    },
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 14;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Mid;
const BYPASS_BRIGHTNESS: Brightness = Brightness::High;
const ATT_DEADBAND: u16 = 32;
const FADER_MOVE_THRESHOLD: u16 = 24;
const HYSTERESIS_VOLTS: f32 = 0.1;
const DOWNBEAT_TICKS: u32 = 96;
const BLINK_MS: u64 = 25;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Control Issues",
    "Shape CV and send it as MIDI control",
    Color::Pink,
    AppIcon::Attenuate,
)
.add_param(Param::Enum {
    name: "Input Mode",
    variants: &["Continuous", "Gate"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Transfer",
    variants: &[
        "Linear",
        "Logarithmic",
        "Exponential",
        "S-Curve",
        "Rectify",
        "Fold",
    ],
})
.add_param(Param::Enum {
    name: "Slew Mode",
    variants: &["Both", "Rise", "Fall"],
})
.add_param(Param::i32 {
    name: "Steps",
    min: 0,
    max: 128,
})
.add_param(Param::i32 {
    name: "Output Min",
    min: 0,
    max: 127,
})
.add_param(Param::i32 {
    name: "Output Max",
    min: 0,
    max: 127,
})
.add_param(Param::f32 {
    name: "Gate Threshold",
    min: 0.0,
    max: 10.0,
})
.add_param(Param::Enum {
    name: "MIDI Rate",
    variants: &["10 Hz", "25 Hz", "50 Hz", "100 Hz", "250 Hz"],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Pink,
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::MidiNrpn)
.add_param(Param::MidiOut);

pub struct Params {
    input_mode: usize,
    range: Range,
    transfer: usize,
    slew_mode: usize,
    steps: i32,
    output_min: i32,
    output_max: i32,
    gate_threshold: f32,
    midi_rate: usize,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    color: Color,
    nrpn: bool,
    midi_out: MidiOut,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            input_mode: usize::from_value(values[0]),
            range: Range::from_value(values[1]),
            transfer: usize::from_value(values[2]),
            slew_mode: usize::from_value(values[3]),
            steps: i32::from_value(values[4]),
            output_min: i32::from_value(values[5]),
            output_max: i32::from_value(values[6]),
            gate_threshold: match values[7] {
                Value::f32(v) => v,
                _ => 2.0,
            },
            midi_rate: usize::from_value(values[8]),
            midi_channel: MidiChannel::from_value(values[9]),
            midi_cc: MidiCc::from_value(values[10]),
            color: Color::from_value(values[11]),
            nrpn: bool::from_value(values[12]),
            midi_out: MidiOut::from_value(values[13]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.input_mode.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.transfer.into()).unwrap();
        vec.push(self.slew_mode.into()).unwrap();
        vec.push(self.steps.into()).unwrap();
        vec.push(self.output_min.into()).unwrap();
        vec.push(self.output_max.into()).unwrap();
        vec.push(self.gate_threshold.into()).unwrap();
        vec.push(self.midi_rate.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    att_saved: u16,
    offset_saved: u16,
    slew_saved: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            // +100 % attenuverter
            att_saved: 4095,
            // 0 % offset
            offset_saved: 2047,
            // 0 ms slew
            slew_saved: 0,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            input_mode: 0,
            range: Range::_0_10V,
            transfer: 0,
            slew_mode: 0,
            steps: 0,
            output_min: 0,
            output_max: 127,
            gate_threshold: 2.0,
            midi_rate: 3, // 100 Hz
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
            color: Color::Pink,
            nrpn: false,
            midi_out: MidiOut::default(),
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
        input_mode,
        range,
        transfer,
        slew_mode,
        steps,
        output_min,
        output_max,
        gate_threshold,
        midi_rate,
        midi_channel,
        midi_cc,
        led_color,
        nrpn,
        midi_out,
    ) = params.query(|p| {
        (
            p.input_mode,
            p.range,
            p.transfer,
            p.slew_mode,
            p.steps,
            p.output_min,
            p.output_max,
            p.gate_threshold,
            p.midi_rate,
            p.midi_channel,
            p.midi_cc,
            p.color,
            p.nrpn,
            p.midi_out,
        )
    });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out, midi_channel, nrpn);
    let input = app.make_in_jack(0, range).await;

    let (lo_cc, hi_cc) = sorted_cc_window(output_min, output_max);
    let out_lo = scale_bits_7_12(lo_cc.into());
    let out_hi = scale_bits_7_12(hi_cc.into());
    let rate_period_ms = midi_rate_period_ms(midi_rate);
    let gate_thresh_counts = volts_to_counts(gate_threshold, range);
    let gate_hyst_counts = volts_to_counts(HYSTERESIS_VOLTS, range).max(1);
    let steps_u16 = if steps <= 1 { 0 } else { steps.clamp(2, 128) as u16 };

    let muted_glob = app.make_global(storage.query(|s| s.muted));
    let bypass_glob = app.make_global(false);
    let latch_glob = app.make_global(LatchLayer::Main);
    let out_glob = app.make_global(0u16);
    let blink_glob = app.make_global(false);
    let force_send_glob = app.make_global(true);
    let slew_sync_glob = app.make_global(false);
    let glob_ticks = app.make_global(0u64);
    let glob_clock_alive = app.make_global(false);
    let glob_blink_req = app.make_global(false);

    if muted_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
    }

    let process_loop = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut slew = shape::SlewState::from(input.get_value());
        let mut schmitt = shape::SchmittTrigger::new();
        let mut last_midi = u16::MAX;
        let mut rate_age_ms = rate_period_ms;
        let mut was_bypassed = false;
        let mut mute_sent = false;

        loop {
            app.delay_millis(1).await;
            rate_age_ms = rate_age_ms.saturating_add(1);

            let shift = buttons.is_shift_pressed();
            let held = buttons.is_button_pressed(0);
            let bypassed = bypass_glob.get();

            let layer = if shift && !held {
                LatchLayer::Alt
            } else if held && !shift {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            latch_glob.set(layer);

            let (att_saved, offset_saved, slew_saved) =
                storage.query(|s| (s.att_saved, s.offset_saved, s.slew_saved));

            let target = match layer {
                LatchLayer::Main => att_saved,
                LatchLayer::Alt => offset_saved,
                LatchLayer::Third => slew_saved,
            };
            if let Some(new_value) = latch.update(fader.get_value(), layer, target) {
                match layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.att_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.offset_saved = new_value);
                    }
                    LatchLayer::Third => {
                        storage.modify_and_save(|s| s.slew_saved = new_value);
                    }
                }
            }

            let att = shape::bipolar_deadband(
                storage.query(|s| s.att_saved),
                ATT_DEADBAND,
            );
            let offset = shape::bipolar_deadband(
                storage.query(|s| s.offset_saved),
                ATT_DEADBAND,
            );
            let slew_fader = storage.query(|s| s.slew_saved);
            let slew_ms = fader_to_slew_ms(slew_fader);

            let cv = input.get_value();
            let gated = if input_mode == 1 {
                if schmitt.update(cv, gate_thresh_counts, gate_hyst_counts) {
                    4095
                } else {
                    0
                }
            } else {
                cv
            };

            if bypassed != was_bypassed || slew_sync_glob.get() {
                slew = shape::SlewState::from(gated);
                was_bypassed = bypassed;
                slew_sync_glob.set(false);
            }

            let processed = if bypassed {
                gated
            } else {
                let after_att = attenuverter(gated, att);
                let after_off = after_att as i32 + (offset as i32 - 2047);
                let shaped = apply_transfer(after_off, transfer);
                let (rise_ms, fall_ms) = slew_times(slew_ms, slew_mode);
                slew = shape::slew_lin_ms(slew, shaped, rise_ms, fall_ms);
                shape::quantize_steps_12bit(slew.value(), steps_u16)
            };

            let windowed = scale_to_window(processed, out_lo, out_hi);
            out_glob.set(windowed);

            let muted = muted_glob.get();
            if muted {
                if !mute_sent {
                    let min_val = out_lo;
                    midi.send_cc(midi_cc, min_val).await;
                    last_midi = midi_gate(min_val, nrpn);
                    mute_sent = true;
                    force_send_glob.set(false);
                }
            } else {
                mute_sent = false;
                let gate_val = midi_gate(windowed, nrpn);
                let due = rate_age_ms >= rate_period_ms;
                let changed = gate_val != last_midi;
                let force = force_send_glob.get();
                if midi_out.is_some() && due && (changed || force) {
                    midi.send_cc(midi_cc, windowed).await;
                    last_midi = gate_val;
                    rate_age_ms = 0;
                    force_send_glob.set(false);
                }
            }

            // LEDs
            match layer {
                LatchLayer::Main => {
                    if range.is_bipolar() {
                        let led = split_unsigned_value(windowed);
                        leds.set(0, Led::Top, led_color, Brightness::Custom(led[0]));
                        leds.set(0, Led::Bottom, led_color, Brightness::Custom(led[1]));
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((windowed / 16) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
                }
                LatchLayer::Alt => {
                    let off = offset as i32 - 2047;
                    let led = split_signed_value(off);
                    leds.set(0, Led::Top, Color::Red, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(led[1]));
                }
                LatchLayer::Third => {
                    leds.set(
                        0,
                        Led::Top,
                        Color::Cyan,
                        Brightness::Custom((slew_fader / 16) as u8),
                    );
                    leds.unset(0, Led::Bottom);
                }
            }

            if !blink_glob.get() {
                if muted {
                    leds.unset(0, Led::Button);
                } else if bypassed {
                    leds.set(0, Led::Button, led_color, BYPASS_BRIGHTNESS);
                } else {
                    leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
                }
            }
        }
    };

    let button_gestures = async {
        loop {
            let shift_on_down = buttons.wait_for_down(0).await;
            if shift_on_down {
                bypass_glob.set(true);
                slew_sync_glob.set(true);
                force_send_glob.set(true);
                buttons.wait_for_up(0).await;
                bypass_glob.set(false);
                slew_sync_glob.set(true);
                force_send_glob.set(true);
                continue;
            }

            let start = fader.get_value();
            let mut moved = false;
            loop {
                match select(buttons.wait_for_up(0), fader.wait_for_change()).await {
                    Either::First(_) => break,
                    Either::Second(_) => {
                        if fader.get_value().abs_diff(start) >= FADER_MOVE_THRESHOLD {
                            moved = true;
                        }
                    }
                }
            }

            if !moved && !buttons.is_shift_pressed() {
                let muted = storage.modify_and_save(|s| {
                    s.muted = !s.muted;
                    s.muted
                });
                muted_glob.set(muted);
                force_send_glob.set(true);
            }
        }
    };

    let clock_drain = async {
        let mut clock = app.use_clock();
        let mut last_tick = 0u64;
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    last_tick = 0;
                    glob_ticks.set(0);
                    glob_clock_alive.set(true);
                    glob_blink_req.set(true);
                }
                ClockEvent::Start => {
                    glob_clock_alive.set(true);
                }
                ClockEvent::Stop => {
                    glob_clock_alive.set(false);
                    blink_glob.set(false);
                }
                ClockEvent::Tick(tick) => {
                    glob_ticks.set(tick);
                    glob_clock_alive.set(true);
                    if tick != last_tick {
                        if tick < last_tick || tick.is_multiple_of(DOWNBEAT_TICKS as u64) {
                            glob_blink_req.set(true);
                        }
                        last_tick = tick;
                    }
                }
            }
        }
    };

    let blink_task = async {
        loop {
            if glob_blink_req.get() {
                glob_blink_req.set(false);
                if glob_clock_alive.get() && !muted_glob.get() && out_glob.get() > out_lo {
                    blink_glob.set(true);
                    leds.unset(0, Led::Button);
                    app.delay_millis(BLINK_MS).await;
                    blink_glob.set(false);
                    if muted_glob.get() {
                        leds.unset(0, Led::Button);
                    } else if bypass_glob.get() {
                        leds.set(0, Led::Button, led_color, BYPASS_BRIGHTNESS);
                    } else {
                        leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
                    }
                }
            }
            app.delay_millis(1).await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let muted = storage.query(|s| s.muted);
                    muted_glob.set(muted);
                    bypass_glob.set(false);
                    slew_sync_glob.set(true);
                    force_send_glob.set(true);
                    blink_glob.set(false);
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join5(process_loop, button_gestures, scene_handler, clock_drain, blink_task).await;
}

fn apply_transfer(value: i32, transfer: usize) -> u16 {
    match transfer {
        1 => Curve::Logarithmic.at(value.clamp(0, 4095) as u16),
        2 => Curve::Exponential.at(value.clamp(0, 4095) as u16),
        3 => shape::s_curve_12bit(value.clamp(0, 4095) as u16),
        4 => {
            let signed = value - 2047;
            (signed.unsigned_abs() as u64 + 2047).min(4095) as u16
        }
        5 => shape::fold_12bit(value),
        _ => Curve::Linear.at(value.clamp(0, 4095) as u16),
    }
}

fn sorted_cc_window(min: i32, max: i32) -> (u8, u8) {
    let a = min.clamp(0, 127) as u8;
    let b = max.clamp(0, 127) as u8;
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn scale_to_window(value: u16, lo: u16, hi: u16) -> u16 {
    if lo >= hi {
        return lo;
    }
    let span = (hi - lo) as u32;
    lo + ((value.min(4095) as u32 * span) / 4095) as u16
}

fn fader_to_slew_ms(fader: u16) -> u32 {
    (fader as u32 * 10_000) / 4095
}

fn slew_times(ms: u32, mode: usize) -> (u32, u32) {
    match mode {
        1 => (ms, 0), // Rise only
        2 => (0, ms), // Fall only
        _ => (ms, ms),
    }
}

fn midi_rate_period_ms(rate_idx: usize) -> u32 {
    match rate_idx {
        0 => 100, // 10 Hz
        1 => 40,  // 25 Hz
        2 => 20,  // 50 Hz
        4 => 4,   // 250 Hz
        _ => 10,  // 100 Hz
    }
}

fn volts_to_counts(volts: f32, range: Range) -> u16 {
    let v = if volts.is_finite() { volts.max(0.0) } else { 0.0 };
    match range {
        Range::_0_5V => ((v.min(5.0) / 5.0) * 4095.0) as u16,
        Range::_Neg5_5V => {
            let pos = v.min(5.0);
            (2047.0 + (pos / 5.0) * 2048.0).clamp(0.0, 4095.0) as u16
        }
        Range::_0_10V => ((v.min(10.0) / 10.0) * 4095.0) as u16,
    }
}

/// CV shaping helpers — kept local so the app PR does not touch `libfp`.
mod shape {
    #[derive(Clone, Copy, Default)]
    pub struct SlewState(u32);

    impl SlewState {
        pub fn value(self) -> u16 {
            (self.0 >> 8) as u16
        }
    }

    impl From<u16> for SlewState {
        fn from(v: u16) -> Self {
            Self((v as u32) << 8)
        }
    }

    /// Smoothstep S-curve on a 12-bit value (`0..=4095`).
    pub fn s_curve_12bit(value: u16) -> u16 {
        let x = value.min(4095) as u64;
        let x2 = x * x;
        let num = 3 * x2 * 4095 - 2 * x2 * x;
        let den = 4095u64 * 4095;
        ((num + den / 2) / den).min(4095) as u16
    }

    /// Wavefold an integer sample into `0..=4095` by repeated reflection.
    pub fn fold_12bit(mut value: i32) -> u16 {
        for _ in 0..64 {
            if (0..=4095).contains(&value) {
                return value as u16;
            }
            if value < 0 {
                value = -value;
            } else {
                value = 8190 - value;
            }
        }
        value.clamp(0, 4095) as u16
    }

    /// Quantize a 12-bit value onto `steps` evenly spaced levels.
    pub fn quantize_steps_12bit(value: u16, steps: u16) -> u16 {
        let value = value.min(4095);
        if steps <= 1 {
            return value;
        }
        let steps = steps as u32;
        let max_idx = steps - 1;
        let idx = (value as u32 * max_idx + 2047) / 4095;
        ((idx * 4095) / max_idx) as u16
    }

    /// Snap an attenuverter/offset fader to exact mid-scale inside a deadband.
    pub fn bipolar_deadband(value: u16, deadband: u16) -> u16 {
        if value.abs_diff(2047) <= deadband {
            2047
        } else {
            value
        }
    }

    /// Schmitt-trigger gate detector with independent high/low thresholds.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SchmittTrigger {
        high: bool,
    }

    impl SchmittTrigger {
        pub const fn new() -> Self {
            Self { high: false }
        }

        pub fn update(&mut self, input: u16, threshold: u16, hysteresis: u16) -> bool {
            let low = threshold.saturating_sub(hysteresis);
            if input >= threshold {
                self.high = true;
            } else if input <= low {
                self.high = false;
            }
            self.high
        }
    }

    fn slew_ms_to_step_fp(ms: u32) -> Option<u32> {
        let ms = ms.min(10_000);
        if ms == 0 {
            return None;
        }
        Some(((4095u32 << 8) + ms / 2) / ms)
    }

    /// Linear slew for a 1 ms processing loop.
    pub fn slew_lin_ms(prev: SlewState, input: u16, rise_ms: u32, fall_ms: u32) -> SlewState {
        let prev = prev.0;
        let input_fp = (input as u32) << 8;

        SlewState(if input_fp > prev {
            match slew_ms_to_step_fp(rise_ms) {
                None => input_fp,
                Some(step) if prev + step < input_fp => prev + step,
                Some(_) => input_fp,
            }
        } else if input_fp < prev {
            match slew_ms_to_step_fp(fall_ms) {
                None => input_fp,
                Some(step) if prev.saturating_sub(step) > input_fp => prev - step,
                Some(_) => input_fp,
            }
        } else {
            input_fp
        })
    }
}
