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
    utils::{attenuate, attenuate_bipolar, split_unsigned_value},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, Param, Range, Value, Waveform,
    APP_MAX_PARAMS,
};

use crate::{
    app::{App, AppStorage, ClockEvent, Led, ManagedStorage, SceneEvent},
    storage::{AppParams, ParamStore},
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

pub static CONFIG: Config<PARAMS> =
    Config::new("LFO", "Multi shape LFO", Color::Yellow, AppIcon::Sine)
        .add_param(Param::Enum {
            name: "Speed",
            variants: &["Normal", "Slow", "Slowest"],
        })
        .add_param(Param::Range {
            name: "Range",
            variants: &[Range::_0_10V, Range::_Neg5_5V],
        })
        .add_param(Param::bool { name: "Send MIDI" })
        .add_param(Param::i32 {
            name: "MIDI Channel",
            min: 1,
            max: 16,
        })
        .add_param(Param::i32 {
            name: "MIDI CC",
            min: 1,
            max: 128,
        });

pub struct Params {
    speed_mult: usize,
    range: Range,
    use_midi: bool,
    midi_channel: i32,
    midi_cc: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            speed_mult: 0,
            range: Range::_Neg5_5V,
            use_midi: false,
            midi_channel: 1,
            midi_cc: 32,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        Some(Self {
            speed_mult: usize::from_value(values[0]),
            range: Range::from_value(values[1]),
            use_midi: bool::from_value(values[2]),
            midi_channel: i32::from_value(values[3]),
            midi_cc: i32::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.speed_mult.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.use_midi.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    clocked: bool,
    layer_attenuation: u16,
    layer_speed: u16,
    wave: Waveform,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            clocked: false,
            layer_attenuation: 4095,
            layer_speed: 2000,
            wave: Waveform::Sine,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
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
    let (range, use_midi, midi_chan, midi_cc) =
        params.query(|p| (p.range, p.use_midi, p.midi_channel, p.midi_cc));

    let speed_mult = 2u32.pow(params.query(|p| p.speed_mult) as u32);
    let output = app.make_out_jack(0, range).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clk = app.use_clock();

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_tick = app.make_global(false);
    let glob_div = app.make_global(24);

    let curve = Curve::Exponential;
    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6];

    let (speed, wave) = storage.query(|s| (s.layer_speed, s.wave));

    let color = get_color_for(wave);

    leds.set(0, Led::Button, color, Brightness::Lower);

    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
    glob_div.set(resolution[speed as usize / 500]);
    let mut count = 0;
    let mut quant_speed: f32 = 6.;
    let mut last_out = 0;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let (sync, wave) = storage.query(|s| (s.clocked, s.wave));

            count += 1;
            if glob_tick.get() {
                // add timeout
                let div = glob_div.get();
                quant_speed = 4095. / ((count * div) as f32 / 24.);
                count = 0;
                glob_tick.set(false);
            }

            let lfo_speed = glob_lfo_speed.get();
            let lfo_pos = glob_lfo_pos.get();

            let next_pos = if sync {
                (lfo_pos + quant_speed / speed_mult as f32) % 4096.0
            } else {
                (lfo_pos + lfo_speed / speed_mult as f32) % 4096.0
            };

            let attenuation = storage.query(|s| s.layer_attenuation);
            let val = if range == Range::_Neg5_5V {
                attenuate_bipolar(wave.at(next_pos as usize), attenuation)
            } else {
                attenuate(wave.at(next_pos as usize), attenuation)
            };

            output.set_value(val);
            if use_midi {
                if last_out / 32 != val / 32 {
                    midi.send_cc(midi_cc as u8, val).await;
                }
                last_out = val;
            }

            let led = if range == Range::_Neg5_5V {
                split_unsigned_value(val)
            } else {
                [(val / 16) as u8, 0]
            };

            let color = get_color_for(wave);

            if sync && next_pos as u16 > 2048 {
                leds.set(0, Led::Button, color, Brightness::Lowest);
            } else {
                leds.set(0, Led::Button, color, Brightness::Lower);
            }

            match latch_active_layer {
                LatchLayer::Main => {
                    leds.set(0, Led::Top, color, Brightness::Custom(led[0]));
                    leds.set(0, Led::Bottom, color, Brightness::Custom(led[1]));
                }
                LatchLayer::Alt => {
                    leds.set(
                        0,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom(((attenuation / 16) / 2) as u8),
                    );
                    leds.unset(0, Led::Bottom);
                }
                _ => unreachable!(),
            }

            glob_lfo_pos.set(next_pos);
        }
    };

    let fut2 = async {
        let mut latch = app.make_latch(fader.get_value());

        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.layer_speed),
                LatchLayer::Alt => storage.query(|s| s.layer_attenuation),
                _ => unreachable!(),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_lfo_speed.set(curve.at(new_value) as f32 * 0.015 + 0.0682);
                        glob_div.set(resolution[new_value as usize / 500]);
                        storage.modify_and_save(|s| s.layer_speed = new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.layer_attenuation = new_value);
                    }
                    _ => unreachable!(),
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;

            if !buttons.is_shift_pressed() {
                let wave = storage.modify_and_save(|s| {
                    s.wave = s.wave.cycle();
                    s.wave
                });

                let color = get_color_for(wave);
                leds.set(0, Led::Button, color, Brightness::Lower);
            } else {
                glob_lfo_pos.set(0.0);
            }
        }
    };

    let fut4 = async {
        loop {
            buttons.wait_for_any_long_press().await;

            if buttons.is_shift_pressed() {
                let clocked = storage.modify_and_save(|s| {
                    s.clocked = !s.clocked;
                    s.clocked
                });
                if clocked {
                    leds.set_mode(0, Led::Button, LedMode::Flash(color, Some(4)));
                }
            }
        }
    };
    let fut5 = async {
        loop {
            match clk.wait_for_event(ClockDivision::_24).await {
                ClockEvent::Tick => {
                    glob_tick.set(true);
                }
                ClockEvent::Reset => {
                    glob_lfo_pos.set(0.0);
                }
                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load_from_scene(scene).await;
                    let speed = storage.query(|s| s.layer_speed);
                    let wave_saved = storage.query(|s| s.wave);

                    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
                    glob_div.set(resolution[speed as usize / 500]);

                    let color = get_color_for(wave_saved);
                    leds.set(0, Led::Button, color, Brightness::Lower);
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join(join5(fut1, fut2, fut3, fut4, scene_handler), fut5).await;
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
