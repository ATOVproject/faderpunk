use embassy_futures::{
    join::{join, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    colors::{PURPLE, RED, TEAL, WHITE, YELLOW},
    latch::LatchLayer,
    utils::{attenuate_bipolar, split_unsigned_value},
    Brightness, Curve,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Range, Waveform};
use smart_leds::RGB8;

use crate::{
    app::{App, AppStorage, ClockEvent, Led, ManagedStorage, SceneEvent},
    storage::ParamStore,
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("LFO", "Wooooosh");

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

pub struct Params {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new([], app.app_id, app.start_channel);
    let params = Params {};

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            storage.load(None).await;
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, _params: &Params, storage: ManagedStorage<Storage>) {
    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clk = app.use_clock();

    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_tick = app.make_global(false);
    let glob_div = app.make_global(24);

    let curve = Curve::Logarithmic;
    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6];

    let (speed, wave) = storage.query(|s| (s.layer_speed, s.wave));

    let color = get_color_for(wave);

    leds.set(0, Led::Button, color, Brightness::Lower);

    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
    glob_div.set(resolution[speed as usize / 500]);
    let mut count = 0;
    let mut quant_speed: f32 = 6.;

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
                (lfo_pos + quant_speed) % 4096.0
            } else {
                (lfo_pos + lfo_speed) % 4096.0
            };

            let attenuation = storage.query(|s| s.layer_attenuation);

            let val = attenuate_bipolar(wave.at(next_pos as usize), attenuation);

            output.set_value(val);
            let led = split_unsigned_value(val);

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
                        RED,
                        Brightness::Custom(((attenuation / 16) / 2) as u8),
                    );
                    leds.unset(0, Led::Bottom);
                }
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
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_lfo_speed.set(curve.at(new_value) as f32 * 0.015 + 0.0682);
                        glob_div.set(resolution[new_value as usize / 500]);
                        storage
                            .modify_and_save(|s| s.layer_speed = new_value, None)
                            .await;
                    }
                    LatchLayer::Alt => {
                        storage
                            .modify_and_save(|s| s.layer_attenuation = new_value, None)
                            .await;
                    }
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;

            if !buttons.is_shift_pressed() {
                let wave = storage
                    .modify_and_save(
                        |s| {
                            s.wave = s.wave.cycle();
                            s.wave
                        },
                        None,
                    )
                    .await;

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
                let clocked = storage
                    .modify_and_save(
                        |s| {
                            s.clocked = !s.clocked;
                            s.clocked
                        },
                        None,
                    )
                    .await;
                if clocked {
                    leds.set_mode(0, Led::Button, LedMode::Flash(color, Some(4)));
                }
            }
        }
    };
    let fut5 = async {
        loop {
            match clk.wait_for_event(24).await {
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
                    storage.load(Some(scene)).await;
                    let speed = storage.query(|s| s.layer_speed);
                    let wave_saved = storage.query(|s| s.wave);

                    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);
                    glob_div.set(resolution[speed as usize / 500]);

                    let color = get_color_for(wave_saved);
                    leds.set(0, Led::Button, color, Brightness::Lower);
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join(join5(fut1, fut2, fut3, fut4, scene_handler), fut5).await;
}

fn get_color_for(wave: Waveform) -> RGB8 {
    match wave {
        Waveform::Sine => YELLOW,
        Waveform::Triangle => PURPLE,
        Waveform::Saw => TEAL,
        Waveform::SawInv => RED,
        Waveform::Rect => WHITE,
    }
}
