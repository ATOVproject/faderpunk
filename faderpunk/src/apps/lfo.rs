use embassy_futures::{
    join::{join, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Duration;
use libfp::{
    constants::{ATOV_BLUE, ATOV_PURPLE, ATOV_RED, ATOV_WHITE, ATOV_YELLOW, LED_LOW, LED_MID},
    utils::{attenuate_bipolar, is_close, split_unsigned_value},
    Curve,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Waveform};

use crate::{
    app::{App, AppStorage, ClockEvent, Led, ManagedStorage, Range, SceneEvent, RGB8},
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

    storage.load(None).await;
    let fader_saved = storage.query(|s| s.fader_saved).await;
    let wave_saved = storage.query(|s| s.wave_saved).await;
    let att_saved = storage.query(|s| s.att_saved).await;
    let clocked = storage.query(|s| s.clocked_saved).await;
    clocked_glob.set(clocked).await;
    att_glob.set(att_saved).await;
    glob_wave.set(wave_saved).await;

    let color = match wave_saved {
        Waveform::Sine => ATOV_YELLOW,
        Waveform::Triangle => ATOV_PURPLE,
        Waveform::Saw => ATOV_BLUE,
        Waveform::SawInv => ATOV_RED,
        Waveform::Rect => ATOV_WHITE,
    };

    leds.set(0, Led::Button, color, LED_MID);

    glob_lfo_speed
        .set(curve.at(fader_saved as usize) as f32 * 0.015 + 0.0682)
        .await;

    let mut count = 0;
    let mut quant_speed: f32 = 6.;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let sync = storage.query(|s| s.clocked_saved).await;

            count += 1;
            if tick_flag.get().await {
                //add timeout
                let div = div_glob.get().await;
                quant_speed = 4095. / ((count * div) as f32 / 24.);
                // info!("speed = {}, count = {}, div = {}", quant_speed, count, div);
                count = 0;
                tick_flag.set(false).await;
            }

            let wave = glob_wave.get().await;
            let lfo_speed = glob_lfo_speed.get().await;
            let lfo_pos = glob_lfo_pos.get().await;

            // let next_pos = (lfo_pos + lfo_speed) % 4096.0;

            let next_pos = if sync {
                (lfo_pos + quant_speed) % 4096.0
            } else {
                (lfo_pos + lfo_speed) % 4096.0
            };

            let att = att_glob.get().await;

            let val = attenuate_bipolar(wave.at(next_pos as usize), att);

            output.set_value(val);
            let led = split_unsigned_value(val);

            let color = match wave {
                Waveform::Sine => ATOV_YELLOW,
                Waveform::Triangle => ATOV_PURPLE,
                Waveform::Saw => ATOV_BLUE,
                Waveform::SawInv => ATOV_RED,
                Waveform::Rect => ATOV_WHITE,
            };

            if sync && next_pos as u16 > 2048 {
                leds.set(0, Led::Button, color, LED_LOW);
            } else {
                leds.set(0, Led::Button, color, LED_MID);
            }

            if !buttons.is_shift_pressed() {
                leds.set(0, Led::Top, color, led[0]);
                leds.set(0, Led::Bottom, color, led[1]);
            } else {
                leds.set(0, Led::Top, ATOV_RED, ((att / 16) / 2) as u8);
                leds.set(0, Led::Bottom, ATOV_RED, 0);
            }

            glob_lfo_pos.set(next_pos).await;

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;

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
                if !latched_glob.get().await && is_close(fader_val, stored_faders) {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    glob_lfo_speed
                        .set(curve.at(fader_val as usize) as f32 * 0.015 + 0.0682)
                        .await;
                    div_glob
                        .set(resolution[(fader_val as usize / 455).clamp(0, 8)])
                        .await;
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
                if !latched_glob.get().await && is_close(fader_val, att_glob.get().await) {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    att_glob.set(fader_val).await;
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
                let mut wave = glob_wave.get().await;
                glob_wave.set(wave.cycle()).await;
                wave = glob_wave.get().await;

                let color = match wave {
                    Waveform::Sine => ATOV_YELLOW,
                    Waveform::Triangle => ATOV_PURPLE,
                    Waveform::Saw => ATOV_BLUE,
                    Waveform::SawInv => ATOV_RED,
                    Waveform::Rect => ATOV_WHITE,
                };
                leds.set(0, Led::Button, color, LED_MID);

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
                glob_lfo_pos.set(0.0).await;
            }
        }
    };

    let fut4 = async {
        loop {
            buttons
                .wait_for_any_long_press(Duration::from_millis(500))
                .await;

            let mut wave = glob_wave.get().await;
            for n in 0..2 {
                wave.cycle();
            }
            glob_wave.set(wave).await;

            let color = match wave {
                Waveform::Sine => ATOV_YELLOW,
                Waveform::Triangle => ATOV_PURPLE,
                Waveform::Saw => ATOV_BLUE,
                Waveform::SawInv => ATOV_RED,
                Waveform::Rect => ATOV_WHITE,
            };

            let clocked = storage
                .modify_and_save(
                    |s| {
                        s.clocked_saved = !s.clocked_saved;
                        s.clocked_saved
                    },
                    None,
                )
                .await;
            clocked_glob.set(clocked).await;
            if clocked {
                leds.set_mode(0, Led::Button, LedMode::Flash(color, Some(4)));
            }
        }
    };
    let fut5 = async {
        loop {
            if let ClockEvent::Tick = clk.wait_for_event(24).await {
                tick_flag.set(true).await;
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
                    att_glob.set(att_saved).await;
                    glob_wave.set(wave_saved).await;

                    glob_lfo_speed
                        .set(curve.at(fader_saved as usize) as f32 * 0.015 + 0.0682)
                        .await;
                    div_glob.set(resolution[fader_saved as usize / 455]).await;

                    let color = match wave_saved {
                        Waveform::Sine => ATOV_YELLOW,
                        Waveform::Triangle => ATOV_PURPLE,
                        Waveform::Saw => ATOV_BLUE,
                        Waveform::SawInv => ATOV_RED,
                        Waveform::Rect => ATOV_WHITE,
                    };
                    leds.set(0, Led::Button, color, LED_MID);
                    latched_glob.set(false).await;
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join(join5(fut1, fut2, fut3, fut4, scene_handler), fut5).await;
}
