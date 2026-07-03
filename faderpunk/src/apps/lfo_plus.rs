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
    app::{App, AppStorage, ClockEvent, Led, ManagedStorage, SceneEvent},
    storage::{AppParams, ParamStore},
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 8;

pub static CONFIG: Config<PARAMS> = Config::new(
    "LFO+",
    "Multi shape LFO with CV input",
    Color::Yellow,
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
.add_param(Param::bool { name: "Grid Lock" });

pub struct Params {
    speed_mult: usize,
    range: Range,
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    color_in: Color,
    nrpn: bool,
    phase_lock: bool,
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
            color_in: Color::from_value(values[4]),
            nrpn: bool::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
            phase_lock: bool::from_value(values[7]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.speed_mult.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color_in.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.phase_lock.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    clocked: bool,
    layer_attenuation: u16,
    layer_speed: u16,
    wave: Waveform,
    in_att: u16,
    in_mute: bool,
    dest: usize,
    out_muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            clocked: false,
            layer_attenuation: 4095,
            layer_speed: 2000,
            wave: Waveform::Sine,
            in_att: 4095,
            in_mute: false,
            dest: 0, // 0 => speed, 1 => phase, 2 => amp, 3 => reset
            out_muted: false,
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
            color_in: Color::Blue,
            nrpn: false,
            phase_lock: true,
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
    let (range, midi_out, midi_chan, midi_cc, color_in, nrpn) = params.query(|p| {
        (
            p.range,
            p.midi_out,
            p.midi_channel,
            p.midi_cc,
            p.color_in,
            p.nrpn,
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

    let midi = app.use_midi_output(midi_out, midi_chan, nrpn);

    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_tick = app.make_global(false);
    let glob_div = app.make_global(24u16);
    let glob_quant_speed = app.make_global(0.07);
    let glob_count = app.make_global(20);
    // Clock tick at which the LFO phase is considered zero. 0 = locked to the
    // clock grid; set to the current tick on a manual/CV reset to run out of phase.
    let glob_phase_origin = app.make_global(0u64);
    // Last tick number seen by clock_handler; read by the input/button futures.
    let glob_ticks = app.make_global(0u64);

    let curve = Curve::Exponential;
    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6];

    let (speed, wave) = storage.query(|s| (s.layer_speed, s.wave));

    let color = get_color_for(wave);

    let glob_out_muted = app.make_global(storage.query(|s| s.out_muted));
    let long_press_fired = app.make_global(false);

    if !glob_out_muted.get() {
        leds.set(1, Led::Button, color, Brightness::Mid);
    }

    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
    glob_div.set(resolution[(speed as usize / 500).clamp(0, 8)]);
    let mut count = 0;

    let mut last_val: u16 = u16::MAX;

    if storage.query(|s| s.in_mute) {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, color_in, Brightness::Mid);
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
            // Re-align to the clock grid whenever the musical division changes.
            glob_phase_origin.set(0);
        }
        glob_quant_speed.set(4096. / (glob_count.get().max(1) as f32 * div as f32));
    };

    let fut1 = async {
        let mut oldinputval = 0;
        loop {
            app.delay_millis(1).await;
            let in_mute = storage.query(|s| s.in_mute);
            let in_val = if in_mute {
                2047
            } else {
                attenuate_bipolar(input.get_value(), storage.query(|s| s.in_att))
            };
            let destination = storage.query(|s| s.dest);

            let speed_offset = if destination == 0 { in_val } else { 2047 };
            time_calc(speed_offset);

            if destination == 3 {
                if in_val >= 2458 && oldinputval < 2458 {
                    glob_phase_origin.set(glob_ticks.get());
                    glob_lfo_pos.set(0.0);
                }
                oldinputval = in_val;
            }

            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let (sync, wave) = storage.query(|s| (s.clocked, s.wave));

            count += 1;
            if glob_tick.get() {
                // add timeout
                glob_count.set(count);
                count = 0;
                glob_tick.set(false);
            }

            let lfo_speed = glob_lfo_speed.get();
            let quant_speed = glob_quant_speed.get();
            let lfo_pos = glob_lfo_pos.get();

            let next_pos = if sync {
                (lfo_pos + quant_speed / speed_mult as f32) % 4096.0
            } else {
                (lfo_pos + lfo_speed / speed_mult as f32) % 4096.0
            };

            let attenuation = (storage.query(|s| s.layer_attenuation) as i16
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
            let val = if range.is_bipolar() {
                attenuate_bipolar(
                    wave.at((next_pos as i16 + phase_offset).rem_euclid(4096) as usize),
                    attenuation,
                )
            } else {
                attenuate(
                    wave.at((next_pos as i16 + phase_offset).rem_euclid(4096) as usize),
                    attenuation,
                )
            };

            let out_muted = glob_out_muted.get();
            let effective_val = if out_muted {
                if range.is_bipolar() {
                    2047
                } else {
                    0
                }
            } else {
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

            let led = if range.is_bipolar() {
                split_unsigned_value(val)
            } else {
                [(val / 16) as u8, 0]
            };

            let color = get_color_for(wave);

            if out_muted {
                leds.unset(1, Led::Button);
            } else if sync && next_pos as u16 > 2048 {
                leds.set(1, Led::Button, color, Brightness::Low);
            } else {
                leds.set(1, Led::Button, color, Brightness::Mid);
            }

            match latch_active_layer {
                LatchLayer::Main => {
                    leds.set(1, Led::Top, color, Brightness::Custom(led[0]));
                    leds.set(1, Led::Bottom, color, Brightness::Custom(led[1]));
                    if in_mute {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, color_in, Brightness::Mid);
                    }
                }
                LatchLayer::Alt => {
                    leds.set(
                        1,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom(((attenuation / 16) / 2) as u8),
                    );
                    leds.unset(1, Led::Bottom);

                    let dest_color = match destination {
                        0 => Color::Yellow,
                        1 => Color::Pink,
                        2 => Color::Cyan,
                        3 => Color::Red,
                        _ => Color::Yellow,
                    };
                    leds.set(0, Led::Button, dest_color, Brightness::Mid);
                }
                LatchLayer::Third => {}
            }
            let led0 = split_unsigned_value(in_val);
            leds.set(0, Led::Top, color_in, Brightness::Custom(led0[0]));
            leds.set(0, Led::Bottom, color_in, Brightness::Custom(led0[1]));

            glob_lfo_pos.set(next_pos);
        }
    };

    let fader_handler = async {
        let mut latch = [
            app.make_latch(fader.get_value_at(0)),
            app.make_latch(fader.get_value_at(1)),
        ];

        loop {
            let chan = fader.wait_for_any_change().await;

            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.in_att),
                    LatchLayer::Alt => 0,
                    LatchLayer::Third => 0,
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.in_att = new_value);
                        }
                        LatchLayer::Alt => {}
                        LatchLayer::Third => {}
                    }
                }
            }
            if chan == 1 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.layer_speed),
                    LatchLayer::Alt => storage.query(|s| s.layer_attenuation),
                    LatchLayer::Third => 0,
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.layer_speed = new_value);
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| s.layer_attenuation = new_value);
                        }
                        LatchLayer::Third => {}
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;

            if !shift {
                if chan == 0 {
                    storage.modify_and_save(|s| {
                        s.in_mute = !s.in_mute;
                    });
                }
                if chan == 1 {
                    long_press_fired.set(false);
                    buttons.wait_for_up(1).await;
                    if !long_press_fired.get() {
                        let wave = storage.modify_and_save(|s| {
                            s.wave = s.wave.cycle();
                            s.wave
                        });
                        if !glob_out_muted.get() {
                            let color = get_color_for(wave);
                            leds.set(1, Led::Button, color, Brightness::Mid);
                        }
                    }
                }
            } else {
                if chan == 0 {
                    storage.modify_and_save(|s| {
                        s.dest = (s.dest + 1) % 4;
                    });
                }
                if chan == 1 {
                    // Offset the phase lock to the current tick so a clocked LFO
                    // can be pushed out of phase; also resets when free-running.
                    glob_phase_origin.set(glob_ticks.get());
                    glob_lfo_pos.set(0.0);
                }
            }
        }
    };

    let long_press_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_long_press().await;
            if chan == 1 && shift {
                let clocked = storage.modify_and_save(|s| {
                    s.clocked = !s.clocked;
                    s.clocked
                });
                if clocked {
                    let current_wave = storage.query(|s| s.wave);
                    let current_color = get_color_for(current_wave);
                    leds.set_mode(1, Led::Button, LedMode::Flash(current_color, Some(4)));
                }
            } else if chan == 1 && !shift {
                long_press_fired.set(true);
                let muted = glob_out_muted.toggle();
                storage.modify_and_save(|s| {
                    s.out_muted = muted;
                });
                if muted {
                    leds.unset(1, Led::Button);
                } else {
                    let wave = storage.query(|s| s.wave);
                    let color = get_color_for(wave);
                    leds.set(1, Led::Button, color, Brightness::Mid);
                }
            }
        }
    };
    let clock_handler = async {
        loop {
            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick(tick) => {
                    glob_ticks.set(tick);
                    if storage.query(|s| s.clocked) && phase_lock {
                        let ticks_per_cycle =
                            (glob_div.get() as u64).saturating_mul(speed_mult as u64);
                        if ticks_per_cycle > 0 {
                            let phase_in_cycle =
                                tick.wrapping_sub(glob_phase_origin.get()) % ticks_per_cycle;
                            glob_lfo_pos
                                .set(phase_in_cycle as f32 * 4096.0 / ticks_per_cycle as f32);
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
                    let (speed, wave_saved, out_muted) =
                        storage.query(|s| (s.layer_speed, s.wave, s.out_muted));

                    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
                    glob_div.set(resolution[(speed as usize / 500).clamp(0, 8)]);
                    glob_out_muted.set(out_muted);

                    if out_muted {
                        leds.unset(1, Led::Button);
                    } else {
                        let color = get_color_for(wave_saved);
                        leds.set(1, Led::Button, color, Brightness::Mid);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join(
        join5(
            fut1,
            fader_handler,
            button_handler,
            long_press_handler,
            scene_handler,
        ),
        clock_handler,
    )
    .await;
}

fn get_color_for(wave: Waveform) -> Color {
    match wave {
        Waveform::Sine => Color::Yellow,
        Waveform::Triangle => Color::Pink,
        Waveform::Saw => Color::Cyan,
        Waveform::SawInv => Color::Red,
        Waveform::Square => Color::White,
    }
}
