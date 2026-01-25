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
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, Waveform, APP_MAX_PARAMS,
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
        .add_param(Param::MidiOut)
        .add_param(Param::MidiChannel {
            name: "MIDI Channel",
        })
        .add_param(Param::MidiCc { name: "MIDI CC" });

pub struct Params {
    speed_mult: usize,
    range: Range,
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            speed_mult: 0,
            range: Range::_Neg5_5V,
            midi_out: MidiOut::default(),
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        Some(Self {
            speed_mult: usize::from_value(values[0]),
            range: Range::from_value(values[1]),
            midi_out: MidiOut::from_value(values[2]),
            midi_channel: MidiChannel::from_value(values[3]),
            midi_cc: MidiCc::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.speed_mult.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
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
    let (range, midi_out, midi_chan, midi_cc) =
        params.query(|p| (p.range, p.midi_out, p.midi_channel, p.midi_cc));

    let speed_mult = 2u32.pow(params.query(|p| p.speed_mult).min(31) as u32);
    let output = app.make_out_jack(0, range).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clk = app.use_clock();

    let midi = app.use_midi_output(midi_out, midi_chan);

    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_tick = app.make_global(false);
    let glob_quant_speed = app.make_global(0.07);
    let glob_count = app.make_global(500);

    let curve = Curve::Exponential;
    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6];

    let wave = storage.query(|s| s.wave);

    let color = get_color_for(wave);

    leds.set(0, Led::Button, color, Brightness::Mid);

    let mut count = 0;
    let mut last_out = 0;

    let update_speed = async || {
        glob_lfo_speed.set((curve.at(storage.query(|s| s.layer_speed)) as f32) * 0.015 + 0.0682);

        let div = resolution[((storage.query(|s| s.layer_speed)) as usize / 500).clamp(0, 8)];
        glob_quant_speed.set(4095. / ((glob_count.get().max(1) as f32 * div as f32) / 24.));
    };

    update_speed().await;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let (sync, wave) = storage.query(|s| (s.clocked, s.wave));

            count += 1;
            if glob_tick.get() {
                // add timeout

                if count < 2000 {
                    glob_count.set(count);
                    update_speed().await
                }
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

            let attenuation = storage.query(|s| s.layer_attenuation);
            let val = if range == Range::_Neg5_5V {
                attenuate_bipolar(wave.at(next_pos as usize), attenuation)
            } else {
                attenuate(wave.at(next_pos as usize), attenuation)
            };

            output.set_value(val);
            if midi_out.is_some() {
                if last_out / 32 != val / 32 {
                    midi.send_cc(midi_cc, val).await;
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
                leds.set(0, Led::Button, color, Brightness::Low);
            } else {
                leds.set(0, Led::Button, color, Brightness::Mid);
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
                LatchLayer::Third => {}
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
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.layer_speed = new_value);
                        update_speed().await;
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.layer_attenuation = new_value);
                    }
                    LatchLayer::Third => {}
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
                leds.set(0, Led::Button, color, Brightness::Mid);
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
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let wave_saved = storage.query(|s| s.wave);
                    update_speed().await;
                    let color = get_color_for(wave_saved);
                    leds.set(0, Led::Button, color, Brightness::Mid);
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
