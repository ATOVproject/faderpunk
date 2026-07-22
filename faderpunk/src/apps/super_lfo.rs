//! Super LFO — morphing dual-osc LFO with CV destinations for form params.
//!
//! UX convention (b): fun/form on Main fader, structure/time on Shift+Fader
//! (Heat Pump / Golden Gate / Grooves family — not LFO+ Speed-on-Main).

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
    utils::{attenuate, attenuate_bipolar, midi_gate, split_unsigned_value},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, Waveform, APP_MAX_PARAMS,
};

use crate::{
    app::{App, AppStorage, ClockEvent, Die, Led, ManagedStorage, SceneEvent},
    storage::{AppParams, ParamStore},
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 10;

const REVERSE_FADE_MS: u16 = 500;
const DEST_COUNT: usize = 8;

/// Morph continuum: soft waves → stepped/chaos.
/// Indices: 0 Sine, 1 Tri, 2 Saw, 3 Square, 4 Walk, 5 S&H, 6 Noise
const MORPH_NODES: usize = 7;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Super LFO",
    "Morphing dual-osc LFO with CV form control",
    Color::Cyan,
    AppIcon::Sine,
)
.add_param(Param::Enum {
    name: "Speed",
    variants: &["Normal", "Slow", "Slowest"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
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
.add_param(Param::MidiNrpn)
.add_param(Param::MidiOut)
.add_param(Param::bool { name: "Grid Lock" })
.add_param(Param::Enum {
    name: "Mix Mode",
    variants: &["Xfade", "Min", "Max", "Sum"],
})
.add_param(Param::Enum {
    name: "Osc B",
    variants: &["Quad", "Octave"],
});

pub struct Params {
    speed_mult: usize,
    range: Range,
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    color: Color,
    nrpn: bool,
    phase_lock: bool,
    mix_mode: usize,
    osc_b: usize,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            speed_mult: usize::from_value(values[0]),
            range: Range::from_value(values[1]),
            midi_channel: MidiChannel::from_value(values[2]),
            midi_cc: MidiCc::from_value(values[3]),
            color: Color::from_value(values[4]),
            nrpn: bool::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
            phase_lock: bool::from_value(values[7]),
            mix_mode: usize::from_value(values[8]),
            osc_b: usize::from_value(values[9]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.speed_mult.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.phase_lock.into()).unwrap();
        vec.push(self.mix_mode.into()).unwrap();
        vec.push(self.osc_b.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    clocked: bool,
    layer_attenuation: u16,
    layer_speed: u16,
    morph: u16,
    skew: u16,
    warp: u16,
    character: u16,
    mix_balance: u16,
    in_att: u16,
    in_mute: bool,
    dest: usize,
    out_muted: bool,
    reversed: bool,
    frozen: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            clocked: false,
            layer_attenuation: 4095,
            layer_speed: 2000,
            morph: 0,
            skew: 2048,
            warp: 0,
            character: 0,
            mix_balance: 2048,
            in_att: 4095,
            in_mute: false,
            dest: 0,
            out_muted: false,
            reversed: false,
            frozen: false,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            speed_mult: 0,
            range: Range::_Neg5_5V,
            midi_out: MidiOut([false, false, false]),
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
            color: Color::Cyan,
            nrpn: false,
            phase_lock: true,
            mix_mode: 0,
            osc_b: 0,
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
    let (range, midi_out, midi_chan, midi_cc, led_color, nrpn, mix_mode, osc_b) =
        params.query(|p| {
            (
                p.range,
                p.midi_out,
                p.midi_channel,
                p.midi_cc,
                p.color,
                p.nrpn,
                p.mix_mode.min(3),
                p.osc_b.min(1),
            )
        });

    let speed_mult = 2u32.pow(params.query(|p| p.speed_mult).min(31) as u32);
    let phase_lock = params.query(|p| p.phase_lock);

    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let output = app.make_out_jack(1, range).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clk = app.use_clock();
    let ticker = clk.get_ticker();
    let midi = app.use_midi_output(midi_out, midi_chan, nrpn);

    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let glob_latch_0 = app.make_global(LatchLayer::Main);
    let glob_latch_1 = app.make_global(LatchLayer::Main);
    let glob_tick = app.make_global(false);
    let glob_div = app.make_global(24u16);
    let glob_quant_speed = app.make_global(0.07);
    let glob_count = app.make_global(20u32);
    let glob_phase_origin = app.make_global(0u64);
    let glob_out_muted = app.make_global(storage.query(|s| s.out_muted));
    let glob_frozen_val = app.make_global(2047u16);
    let glob_mix_mode = app.make_global(mix_mode);
    let long_press_0 = app.make_global(false);
    let long_press_1 = app.make_global(false);
    let fader_moved_0 = app.make_global(false);
    let fader_moved_1 = app.make_global(false);
    let glob_reverse_fade = app.make_global(0u16);
    let glob_reverse_fade_up = app.make_global(false);
    let glob_shift_focus = app.make_global(0xffu8);
    let die = app.use_die();
    let glob_chaos = app.make_global(MorphChaos::new());

    let curve = Curve::Exponential;
    let resolution = [384u16, 192, 96, 48, 24, 16, 12, 8, 6];

    let speed = storage.query(|s| s.layer_speed);
    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
    glob_div.set(resolution[(speed as usize / 500).clamp(0, 8)]);

    let mut count = 0u32;
    let mut last_val: u16 = u16::MAX;
    let mut oldinputval = 0u16;

    if storage.query(|s| s.in_mute) {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }
    if !glob_out_muted.get() {
        leds.set(
            1,
            Led::Button,
            morph_color(storage.query(|s| s.morph)),
            Brightness::Mid,
        );
    }

    let time_calc = |offset: u16| {
        let layer_speed = storage.query(|s| s.layer_speed) as u32;
        let offset_u32 = offset as u32;
        let sum = layer_speed.saturating_add(offset_u32);

        glob_lfo_speed
            .set((curve.at(layer_speed as u16) as f32 + offset as f32 - 2047.0) * 0.015 + 0.0682);

        let index_val = sum.saturating_sub(2047).min(4095) as usize / 500;
        let div = resolution[index_val.clamp(0, 8)];
        if div != glob_div.get() {
            glob_div.set(div);
            glob_phase_origin.set(0);
        }
        glob_quant_speed.set(4096. / (glob_count.get().max(1) as f32 * div as f32));
    };

    let fut_audio = async {
        loop {
            app.delay_millis(1).await;

            // Latch layers per channel (Button+Fader = Third).
            let latch_0 = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            let latch_1 = if buttons.is_shift_pressed() && !buttons.is_button_pressed(1) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(1) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_0.set(latch_0);
            glob_latch_1.set(latch_1);
            if !buttons.is_shift_pressed() {
                glob_shift_focus.set(0xff);
            }

            // Reverse fade on Ch1 button.
            let fade_left = glob_reverse_fade.get();
            if fade_left > 0 {
                let elapsed = REVERSE_FADE_MS.saturating_sub(fade_left);
                let bright = if glob_reverse_fade_up.get() {
                    ((elapsed as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                } else {
                    (((REVERSE_FADE_MS - elapsed) as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                };
                leds.set(1, Led::Button, Color::White, Brightness::Custom(bright));
                let next = fade_left.saturating_sub(1);
                glob_reverse_fade.set(next);
            }

            let in_mute = storage.query(|s| s.in_mute);
            let in_val = if in_mute {
                2047
            } else {
                attenuate_bipolar(input.get_value(), storage.query(|s| s.in_att))
            };
            let destination = storage.query(|s| s.dest).min(DEST_COUNT - 1);

            let speed_offset = if destination == 0 { in_val } else { 2047 };
            time_calc(speed_offset);

            if destination == 3 {
                if in_val >= 2458 && oldinputval < 2458 {
                    glob_phase_origin.set(ticker());
                    glob_lfo_pos.set(0.0);
                }
                oldinputval = in_val;
            }

            count = count.saturating_add(1);
            if glob_tick.get() {
                glob_count.set(count);
                count = 0;
                glob_tick.set(false);
            }

            let (
                sync,
                reversed,
                frozen,
                morph_base,
                skew_base,
                warp_base,
                character,
                attenuation_base,
                mix_balance,
            ) = storage.query(|s| {
                (
                    s.clocked,
                    s.reversed,
                    s.frozen,
                    s.morph,
                    s.skew,
                    s.warp,
                    s.character,
                    s.layer_attenuation,
                    s.mix_balance,
                )
            });

            let cv_delta = in_val as i32 - 2047;
            let morph = cv_mod_u16(morph_base, destination == 4, cv_delta);
            let skew = cv_mod_u16(skew_base, destination == 5, cv_delta);
            let warp = cv_mod_u16(warp_base, destination == 6, cv_delta);
            let character = cv_mod_u16(character, destination == 7, cv_delta);

            let layer_speed = storage.query(|s| s.layer_speed);
            let (eff_skew, eff_warp) = apply_character(skew, warp, character, layer_speed);

            let lfo_speed = glob_lfo_speed.get();
            let quant_speed = glob_quant_speed.get();
            let lfo_pos = glob_lfo_pos.get();

            let step = if sync {
                quant_speed / speed_mult as f32
            } else {
                lfo_speed / speed_mult as f32
            };

            let next_pos = if frozen {
                lfo_pos
            } else if reversed {
                let mut p = lfo_pos - step;
                if p < 0.0 {
                    p += 4096.0;
                }
                p % 4096.0
            } else {
                (lfo_pos + step) % 4096.0
            };

            let attenuation = (attenuation_base as i16
                + if destination == 2 {
                    (in_val as i16 - 2047) * 2
                } else {
                    0
                })
            .clamp(0, 4095) as u16;

            let phase_offset: i16 = if destination == 1 {
                (in_val as i16 - 2047) * 2
            } else {
                0
            };

            let phase_a = (next_pos as i16 + phase_offset).rem_euclid(4096) as usize;
            let phase_b = if osc_b == 0 {
                (phase_a + 1024) % 4096
            } else {
                (phase_a * 2) % 4096
            };

            // Evolve random-walk once per tick (not per morph lerp endpoint).
            {
                let mut chaos = glob_chaos.get();
                chaos.tick_walks(&die);
                glob_chaos.set(chaos);
            }

            let sample_a = {
                let mut chaos = glob_chaos.get();
                let s = morph_sample(phase_a, morph, eff_skew, eff_warp, 0, &mut chaos, &die);
                glob_chaos.set(chaos);
                s
            };
            let sample_b = {
                let mut chaos = glob_chaos.get();
                let s = morph_sample(phase_b, morph, eff_skew, eff_warp, 1, &mut chaos, &die);
                glob_chaos.set(chaos);
                s
            };
            let mixed = mix_samples(sample_a, sample_b, glob_mix_mode.get(), mix_balance);

            let val = if range.is_bipolar() {
                attenuate_bipolar(mixed, attenuation)
            } else {
                attenuate(mixed, attenuation)
            };

            let out_muted = glob_out_muted.get();
            let effective_val = if frozen && !out_muted {
                glob_frozen_val.get()
            } else if out_muted {
                if range.is_bipolar() {
                    2047
                } else {
                    0
                }
            } else {
                glob_frozen_val.set(val);
                val
            };

            output.set_value(effective_val);
            if midi_out.is_some() {
                let gate_val = midi_gate(effective_val, nrpn);
                if gate_val != last_val {
                    midi.send_cc(midi_cc, effective_val).await;
                    last_val = gate_val;
                }
            }

            // LEDs — update every ms (clock tick + fader moves covered).
            // Edit latch stays global Shift=Alt; *display* Alt only on shift-focus channel.
            let shift_focus = glob_shift_focus.get();
            let show_0 = display_latch(latch_0, 0, shift_focus);
            let show_1 = display_latch(latch_1, 1, shift_focus);

            let led = if range.is_bipolar() {
                split_unsigned_value(effective_val)
            } else {
                [(effective_val / 16) as u8, 0]
            };
            let shape_color = morph_color(morph);

            // Ch1 button = output presence (app color), not morph shape.
            if fade_left == 0 {
                if out_muted {
                    leds.unset(1, Led::Button);
                } else if sync && next_pos as u16 > 2048 {
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Mid);
                }
            }

            // Ch1 Top/Bottom: Main = output level; Alt = red speed (only if focused); Third = character
            match show_1 {
                LatchLayer::Main => {
                    leds.set(1, Led::Top, led_color, Brightness::Custom(led[0]));
                    leds.set(1, Led::Bottom, led_color, Brightness::Custom(led[1]));
                }
                LatchLayer::Alt => {
                    let speed_bright = (storage.query(|s| s.layer_speed) / 16) as u8;
                    leds.set(1, Led::Top, Color::Red, Brightness::Custom(speed_bright));
                    leds.unset(1, Led::Bottom);
                }
                LatchLayer::Third => {
                    let zone = (storage.query(|s| s.character) / 1366).min(2);
                    let zone_color = match zone {
                        0 => Color::Blue,
                        1 => Color::Yellow,
                        _ => Color::Orange,
                    };
                    leds.set(1, Led::Top, zone_color, Brightness::Mid);
                    leds.unset(1, Led::Bottom);
                }
            }

            // Ch0: Main = morph shape + amount; Alt = red att (if focused); Third = skew zones
            match show_0 {
                LatchLayer::Main => {
                    let morph_bright = (morph / 16) as u8;
                    leds.set(0, Led::Top, shape_color, Brightness::Custom(morph_bright));
                    let led0 = split_unsigned_value(in_val);
                    leds.set(0, Led::Bottom, led_color, Brightness::Custom(led0[1]));
                    if frozen {
                        leds.set(0, Led::Button, Color::White, Brightness::Low);
                    } else if in_mute {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, shape_color, Brightness::Mid);
                    }
                }
                LatchLayer::Alt => {
                    let att_bright = (storage.query(|s| s.in_att) / 16) as u8;
                    leds.set(0, Led::Top, Color::Red, Brightness::Custom(att_bright));
                    leds.unset(0, Led::Bottom);
                    leds.set(0, Led::Button, dest_color(destination), Brightness::Mid);
                }
                LatchLayer::Third => {
                    let zone = (storage.query(|s| s.skew) / 1366).min(2);
                    let zone_color = match zone {
                        0 => Color::Cyan,
                        1 => Color::Pink,
                        _ => Color::Violet,
                    };
                    leds.set(0, Led::Top, zone_color, Brightness::Mid);
                    leds.unset(0, Led::Bottom);
                }
            }

            if !frozen {
                glob_lfo_pos.set(next_pos);
            }
        }
    };

    let fader_handler = async {
        let mut latch = [
            app.make_latch(fader.get_value_at(0)),
            app.make_latch(fader.get_value_at(1)),
        ];

        loop {
            let chan = fader.wait_for_any_change().await;
            if chan == 0 {
                let latch_layer = glob_latch_0.get();
                if latch_layer == LatchLayer::Alt {
                    glob_shift_focus.set(0);
                }
                if latch_layer == LatchLayer::Third {
                    fader_moved_0.set(true);
                }
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.morph),
                    LatchLayer::Alt => storage.query(|s| s.in_att),
                    LatchLayer::Third => storage.query(|s| s.skew),
                };
                if let Some(new_value) =
                    latch[0].update(fader.get_value_at(0), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.morph = new_value);
                        }
                        LatchLayer::Alt => {
                            glob_shift_focus.set(0);
                            storage.modify_and_save(|s| s.in_att = new_value);
                        }
                        LatchLayer::Third => {
                            fader_moved_0.set(true);
                            storage.modify_and_save(|s| s.skew = new_value);
                        }
                    }
                }
            } else if chan == 1 {
                let latch_layer = glob_latch_1.get();
                if latch_layer == LatchLayer::Alt {
                    glob_shift_focus.set(1);
                }
                if latch_layer == LatchLayer::Third {
                    fader_moved_1.set(true);
                }
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.warp),
                    LatchLayer::Alt => storage.query(|s| s.layer_speed),
                    LatchLayer::Third => storage.query(|s| s.character),
                };
                if let Some(new_value) =
                    latch[1].update(fader.get_value_at(1), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.warp = new_value);
                        }
                        LatchLayer::Alt => {
                            glob_shift_focus.set(1);
                            storage.modify_and_save(|s| s.layer_speed = new_value);
                            time_calc(2047);
                        }
                        LatchLayer::Third => {
                            fader_moved_1.set(true);
                            storage.modify_and_save(|s| s.character = new_value);
                        }
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;

            if chan == 0 {
                if shift {
                    long_press_0.set(false);
                    buttons.wait_for_up(0).await;
                    if !long_press_0.get() {
                        // Shift+short Ch0: cycle CV destination
                        storage.modify_and_save(|s| {
                            s.dest = (s.dest + 1) % DEST_COUNT;
                        });
                    }
                } else {
                    long_press_0.set(false);
                    fader_moved_0.set(false);
                    buttons.wait_for_up(0).await;
                    if !long_press_0.get() {
                        // Short: freeze toggle
                        let frozen = storage.modify_and_save(|s| {
                            s.frozen = !s.frozen;
                            s.frozen
                        });
                        if frozen {
                            leds.set(0, Led::Button, Color::White, Brightness::Low);
                        }
                    } else if !fader_moved_0.get() {
                        // Long without third-layer fader move: in mute
                        storage.modify_and_save(|s| {
                            s.in_mute = !s.in_mute;
                        });
                    }
                }
            } else if chan == 1 {
                if shift {
                    long_press_1.set(false);
                    buttons.wait_for_up(1).await;
                    if !long_press_1.get() {
                        // Shift+short: phase reverse + white fade
                        let reversed = storage.modify_and_save(|s| {
                            s.reversed = !s.reversed;
                            s.reversed
                        });
                        glob_reverse_fade_up.set(!reversed);
                        glob_reverse_fade.set(REVERSE_FADE_MS);
                    }
                } else {
                    long_press_1.set(false);
                    fader_moved_1.set(false);
                    buttons.wait_for_up(1).await;
                    if !long_press_1.get() {
                        // Short: phase reset
                        glob_phase_origin.set(ticker());
                        glob_lfo_pos.set(0.0);
                    } else if !fader_moved_1.get() {
                        // Long without third fader: out mute
                        let muted = glob_out_muted.toggle();
                        storage.modify_and_save(|s| {
                            s.out_muted = muted;
                        });
                        if muted {
                            leds.unset(1, Led::Button);
                        }
                    }
                }
            }
        }
    };

    let long_press_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_long_press().await;
            if chan == 0 {
                long_press_0.set(true);
                if shift {
                    // Shift+long Ch0: cycle mix mode
                    let mode = (glob_mix_mode.get() + 1) % 4;
                    glob_mix_mode.set(mode);
                    leds.set_mode(
                        0,
                        Led::Button,
                        LedMode::Flash(dest_color(mode + 4), Some(3)),
                    );
                }
            } else if chan == 1 {
                long_press_1.set(true);
                if shift {
                    let clocked = storage.modify_and_save(|s| {
                        s.clocked = !s.clocked;
                        s.clocked
                    });
                    if clocked {
                        let color = morph_color(storage.query(|s| s.morph));
                        leds.set_mode(1, Led::Button, LedMode::Flash(color, Some(4)));
                    }
                }
            }
        }
    };

    let clock_handler = async {
        loop {
            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick => {
                    if storage.query(|s| s.clocked) && phase_lock && !storage.query(|s| s.frozen) {
                        let ticks_per_cycle =
                            (glob_div.get() as u64).saturating_mul(speed_mult as u64);
                        if ticks_per_cycle > 0 {
                            let phase_in_cycle =
                                ticker().wrapping_sub(glob_phase_origin.get()) % ticks_per_cycle;
                            let mut pos = phase_in_cycle as f32 * 4096.0 / ticks_per_cycle as f32;
                            if storage.query(|s| s.reversed) {
                                pos = (4096.0 - pos) % 4096.0;
                            }
                            glob_lfo_pos.set(pos);
                        }
                    }
                    glob_tick.set(true);
                }
                ClockEvent::Reset => {
                    glob_phase_origin.set(0);
                    glob_lfo_pos.set(0.0);
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
                    let (speed, out_muted, morph) =
                        storage.query(|s| (s.layer_speed, s.out_muted, s.morph));
                    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
                    glob_div.set(resolution[(speed as usize / 500).clamp(0, 8)]);
                    glob_out_muted.set(out_muted);
                    if out_muted {
                        leds.unset(1, Led::Button);
                    } else {
                        leds.set(1, Led::Button, morph_color(morph), Brightness::Mid);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join(
        join5(
            fut_audio,
            fader_handler,
            button_handler,
            long_press_handler,
            scene_handler,
        ),
        clock_handler,
    )
    .await;
}

fn cv_mod_u16(base: u16, active: bool, cv_delta: i32) -> u16 {
    if !active {
        return base;
    }
    (base as i32 + cv_delta * 2).clamp(0, 4095) as u16
}

/// Edit layer may be Alt on both channels while Shift is held; LED Alt preview
/// only on the channel that last moved its fader (`shift_focus`).
fn display_latch(edit: LatchLayer, chan: u8, shift_focus: u8) -> LatchLayer {
    match edit {
        LatchLayer::Alt if shift_focus != chan => LatchLayer::Main,
        other => other,
    }
}

fn apply_character(skew: u16, warp: u16, character: u16, speed: u16) -> (u16, u16) {
    let boost = (character as u32 * speed as u32) / 4095;
    (
        (skew as u32 + boost).min(4095) as u16,
        (warp as u32 + boost).min(4095) as u16,
    )
}

fn warp_phase(phase: usize, warp: u16) -> usize {
    if warp == 0 {
        return phase % 4096;
    }
    let t = (phase % 4096) as f32 / 4096.0;
    let amount = warp as f32 / 4095.0;
    // Smoothstep blend toward ease-in/out time feel
    let eased = t * t * (3.0 - 2.0 * t);
    let out = t * (1.0 - amount) + eased * amount;
    (out * 4095.0) as usize
}

fn skew_phase(phase: usize, skew: u16) -> usize {
    // Center (2048) = linear; low/high lean the duty/asymmetry
    let t = (phase % 4096) as f32 / 4096.0;
    let s = (skew as f32 / 4095.0 - 0.5) * 2.0; // -1..1
    let warped = if s >= 0.0 {
        libm::powf(t, 1.0 + s)
    } else {
        1.0 - libm::powf(1.0 - t, 1.0 - s)
    };
    (warped.clamp(0.0, 1.0) * 4095.0) as usize
}

#[derive(Clone, Copy)]
struct MorphChaos {
    walk_a: i32,
    walk_b: i32,
    sh_a: u16,
    sh_b: u16,
    sh_bucket_a: u16,
    sh_bucket_b: u16,
}

impl MorphChaos {
    fn new() -> Self {
        Self {
            walk_a: 2048,
            walk_b: 2048,
            sh_a: 2048,
            sh_b: 2048,
            sh_bucket_a: 0xffff,
            sh_bucket_b: 0xffff,
        }
    }

    fn tick_walks(&mut self, die: &Die) {
        // Gentle drift (~±3 at 1 kHz audio tick).
        let step_a = (die.roll() as i32 % 7) - 3;
        let step_b = (die.roll() as i32 % 7) - 3;
        self.walk_a = (self.walk_a + step_a).clamp(0, 4095);
        self.walk_b = (self.walk_b + step_b).clamp(0, 4095);
    }
}

fn classic_wave(node: usize, phase: usize) -> Option<u16> {
    let w = match node {
        0 => Waveform::Sine,
        1 => Waveform::Triangle,
        2 => Waveform::Saw,
        3 => Waveform::Square,
        _ => return None,
    };
    Some(w.at(phase))
}

fn chaos_sample(node: usize, phase: usize, osc: usize, chaos: &mut MorphChaos, die: &Die) -> u16 {
    match node {
        4 => {
            if osc == 0 {
                chaos.walk_a as u16
            } else {
                chaos.walk_b as u16
            }
        }
        5 => {
            // S&H — new level every 1/16 of the cycle (phase bucket).
            let bucket = (phase / 256) as u16;
            let (sh, last) = if osc == 0 {
                (&mut chaos.sh_a, &mut chaos.sh_bucket_a)
            } else {
                (&mut chaos.sh_b, &mut chaos.sh_bucket_b)
            };
            if bucket != *last {
                *last = bucket;
                *sh = die.roll();
            }
            *sh
        }
        _ => die.roll(),
    }
}

fn node_sample(node: usize, phase: usize, osc: usize, chaos: &mut MorphChaos, die: &Die) -> u16 {
    classic_wave(node, phase).unwrap_or_else(|| chaos_sample(node, phase, osc, chaos, die))
}

fn morph_sample(
    phase: usize,
    morph: u16,
    skew: u16,
    warp: u16,
    osc: usize,
    chaos: &mut MorphChaos,
    die: &Die,
) -> u16 {
    let p = skew_phase(warp_phase(phase, warp), skew);
    let segments = MORPH_NODES - 1;
    let seg_size = 4096 / segments;
    let seg = ((morph as usize) / seg_size).min(segments - 1);
    let frac = (morph as usize) % seg_size;
    let a = node_sample(seg, p, osc, chaos, die) as i32;
    let b = node_sample(seg + 1, p, osc, chaos, die) as i32;
    (a + (b - a) * frac as i32 / seg_size as i32).clamp(0, 4095) as u16
}

fn mix_samples(a: u16, b: u16, mode: usize, balance: u16) -> u16 {
    match mode {
        1 => a.min(b),
        2 => a.max(b),
        3 => ((a as u32 + b as u32) / 2) as u16,
        _ => {
            // Xfade
            let t = balance as i32;
            let out = (a as i32 * (4095 - t) + b as i32 * t) / 4095;
            out.clamp(0, 4095) as u16
        }
    }
}

fn morph_color(morph: u16) -> Color {
    let segments = MORPH_NODES - 1;
    let seg_size = 4096 / segments;
    let seg = ((morph as usize) / seg_size).min(segments - 1);
    color_for_node(seg)
}

fn color_for_node(node: usize) -> Color {
    match node {
        0 => Color::Yellow, // Sine
        1 => Color::Pink,   // Tri
        2 => Color::Cyan,   // Saw
        3 => Color::White,  // Square
        4 => Color::Orange, // Rand walk
        5 => Color::Violet, // S&H
        _ => Color::Red,    // Noise
    }
}

fn dest_color(dest: usize) -> Color {
    match dest {
        0 => Color::Yellow, // Speed
        1 => Color::Pink,   // Phase
        2 => Color::Cyan,   // Amp
        3 => Color::Red,    // Reset
        4 => Color::Orange, // Morph
        5 => Color::Violet, // Skew
        6 => Color::Green,  // Warp
        7 => Color::Rose,   // Character
        _ => Color::Yellow,
    }
}
