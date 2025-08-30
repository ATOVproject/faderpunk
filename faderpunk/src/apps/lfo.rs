use embassy_futures::{
    join::{join, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    colors::{PURPLE, RED, TEAL, WHITE, YELLOW},
    utils::{attenuate_bipolar, is_close, split_unsigned_value},
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
    fader_saved: u16,
    wave_saved: Waveform,
    att_saved: u16,
    clocked_saved: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 2000,
            wave_saved: Waveform::Sine,
            att_saved: 4095,
            clocked_saved: false,
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
    let glob_wave = app.make_global(Waveform::Sine);
    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global(false);
    let clocked_glob = app.make_global(false);
    let tick_flag = app.make_global(false);
    let div_glob = app.make_global(24);

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clk = app.use_clock();

    let curve = Curve::Logarithmic;
    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6];

    let mut shift_old = false;

    let fader_saved = storage.query(|s| s.fader_saved).await;
    let wave_saved = storage.query(|s| s.wave_saved).await;
    let att_saved = storage.query(|s| s.att_saved).await;
    let clocked = storage.query(|s| s.clocked_saved).await;
    clocked_glob.set(clocked);
    att_glob.set(att_saved);
    glob_wave.set(wave_saved);

    let color = match wave_saved {
        Waveform::Sine => YELLOW,
        Waveform::Triangle => PURPLE,
        Waveform::Saw => TEAL,
        Waveform::SawInv => RED,
        Waveform::Rect => WHITE,
    };

    leds.set(0, Led::Button, color, Brightness::Lower);

    glob_lfo_speed.set(curve.at(fader_saved) as f32 * 0.015 + 0.0682);
    div_glob.set(resolution[fader_saved as usize / 500]);
    let mut count = 0;
    let mut quant_speed: f32 = 6.;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let sync = storage.query(|s| s.clocked_saved).await;

            count += 1;
            if tick_flag.get() {
                //add timeout
                let div = div_glob.get();
                quant_speed = 4095. / ((count * div) as f32 / 24.);
                // info!("speed = {}, count = {}, div = {}", quant_speed, count, div);
                count = 0;
                tick_flag.set(false);
            }

            let wave = glob_wave.get();
            let lfo_speed = glob_lfo_speed.get();
            let lfo_pos = glob_lfo_pos.get();

            // let next_pos = (lfo_pos + lfo_speed) % 4096.0;

            let next_pos = if sync {
                (lfo_pos + quant_speed) % 4096.0
            } else {
                (lfo_pos + lfo_speed) % 4096.0
            };

            let att = att_glob.get();

            let val = attenuate_bipolar(wave.at(next_pos as usize), att);

            output.set_value(val);
            let led = split_unsigned_value(val);

            let color = get_color_for(wave);

            if sync && next_pos as u16 > 2048 {
                leds.set(0, Led::Button, color, Brightness::Lowest);
            } else {
                leds.set(0, Led::Button, color, Brightness::Lower);
            }

            if !buttons.is_shift_pressed() {
                leds.set(0, Led::Top, color, Brightness::Custom(led[0]));
                leds.set(0, Led::Bottom, color, Brightness::Custom(led[1]));
            } else {
                leds.set(0, Led::Top, RED, Brightness::Custom(((att / 16) / 2) as u8));
                leds.unset(0, Led::Bottom);
            }

            glob_lfo_pos.set(next_pos);

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = false;
            }
        }
    };

    let fut2 = async {
        loop {
            fader.wait_for_change().await;
            let fader_val = fader.get_value();
            let stored_faders = storage.query(|s| s.fader_saved).await;

            if !buttons.is_shift_pressed() {
                if !latched_glob.get() && is_close(fader_val, stored_faders) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    glob_lfo_speed.set(curve.at(fader_val) as f32 * 0.015 + 0.0682);
                    div_glob.set(resolution[fader_val as usize / 500]);
                    // info!("div = {}", div_glob.get().await);

                    storage
                        .modify_and_save(
                            |s| {
                                s.fader_saved = fader_val;
                                s.fader_saved
                            },
                            None,
                        )
                        .await;
                }
            } else {
                if !latched_glob.get() && is_close(fader_val, att_glob.get()) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    att_glob.set(fader_val);
                    storage
                        .modify_and_save(
                            |s| {
                                s.att_saved = fader_val;
                                s.att_saved
                            },
                            None,
                        )
                        .await;
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;

            if !buttons.is_shift_pressed() {
                let mut wave = glob_wave.get();
                glob_wave.set(wave.cycle());
                wave = glob_wave.get();

                let color = get_color_for(wave);
                leds.set(0, Led::Button, color, Brightness::Lower);

                storage
                    .modify_and_save(
                        |s| {
                            s.wave_saved = wave;
                            s.wave_saved
                        },
                        None,
                    )
                    .await;
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
                            s.clocked_saved = !s.clocked_saved;
                            s.clocked_saved
                        },
                        None,
                    )
                    .await;
                clocked_glob.set(clocked);
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
                    tick_flag.set(true);
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
                    let fader_saved = storage.query(|s| s.fader_saved).await;
                    let wave_saved = storage.query(|s| s.wave_saved).await;
                    let att_saved = storage.query(|s| s.att_saved).await;
                    att_glob.set(att_saved);
                    glob_wave.set(wave_saved);

                    glob_lfo_speed.set(curve.at(fader_saved) as f32 * 0.015 + 0.0682);
                    div_glob.set(resolution[fader_saved as usize / 500]);

                    let color = get_color_for(wave_saved);
                    leds.set(0, Led::Button, color, Brightness::Lower);
                    latched_glob.set(false);
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
