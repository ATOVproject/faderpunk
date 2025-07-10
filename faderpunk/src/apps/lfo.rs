// todo
// Add SAVING
// add latching

use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::{
    app::{colors::RED, App, AppStorage, Led, ManagedStorage, Range, SceneEvent, RGB8},
    storage::ParamStore,
};
use config::{Config, Waveform};
use libfp::constants::CURVE_LOG;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("LFO", "Wooooosh");

#[derive(Serialize, Deserialize)]

pub struct Storage {
    fader_saved: u16,
    wave_saved: Waveform,
    att_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 2000,
            wave_saved: Waveform::Sine,
            att_saved: 4095,
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

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let mut shift_old = false;

    storage.load(None).await;
    let fader_saved = storage.query(|s| s.fader_saved).await;
    let wave_saved = storage.query(|s| s.wave_saved).await;
    let att_saved = storage.query(|s| s.att_saved).await;
    att_glob.set(att_saved).await;
    glob_wave.set(wave_saved).await;

    let color = match wave_saved {
        Waveform::Sine => RGB8 {
            r: 243,
            g: 191,
            b: 78,
        },
        Waveform::Triangle => RGB8 {
            r: 188,
            g: 77,
            b: 216,
        },
        Waveform::Saw => RGB8 {
            r: 78,
            g: 243,
            b: 243,
        },
        Waveform::Rect => RGB8 {
            r: 250,
            g: 250,
            b: 250,
        },
    };

    leds.set(0, Led::Button, color, 75);

    glob_lfo_speed
        .set(CURVE_LOG[fader_saved as usize] as f32 * 0.015 + 0.0682)
        .await;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let wave = glob_wave.get().await;
            let lfo_speed = glob_lfo_speed.get().await;
            let lfo_pos = glob_lfo_pos.get().await;
            let next_pos = (lfo_pos + lfo_speed) % 4096.0;
            let att = att_glob.get().await;

            let val = attenuate(wave.at(next_pos as usize), att);

            output.set_value(val);

            if !buttons.is_shift_pressed() {
                leds.set(0, Led::Top, color, ((val as f32 / 16.0) / 2.0) as u8);
                leds.set(
                    0,
                    Led::Bottom,
                    color,
                    ((255.0 - (val as f32) / 16.0) / 2.0) as u8,
                );
            } else {
                leds.set(0, Led::Top, RED, ((att / 16) / 2) as u8);
                leds.set(0, Led::Bottom, RED, 0);
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
                        .set(CURVE_LOG[fader_val as usize] as f32 * 0.015 + 0.0682)
                        .await;
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
                    Waveform::Sine => RGB8 {
                        r: 243,
                        g: 191,
                        b: 78,
                    },
                    Waveform::Triangle => RGB8 {
                        r: 188,
                        g: 77,
                        b: 216,
                    },
                    Waveform::Saw => RGB8 {
                        r: 78,
                        g: 243,
                        b: 243,
                    },
                    Waveform::Rect => RGB8 {
                        r: 250,
                        g: 250,
                        b: 250,
                    },
                };
                leds.set(0, Led::Button, color, 75);

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
                        .set(CURVE_LOG[fader_saved as usize] as f32 * 0.015 + 0.0682)
                        .await;

                    let color = match wave_saved {
                        Waveform::Sine => RGB8 {
                            r: 243,
                            g: 191,
                            b: 78,
                        },
                        Waveform::Triangle => RGB8 {
                            r: 188,
                            g: 77,
                            b: 216,
                        },
                        Waveform::Saw => RGB8 {
                            r: 78,
                            g: 243,
                            b: 243,
                        },
                        Waveform::Rect => RGB8 {
                            r: 250,
                            g: 250,
                            b: 250,
                        },
                    };
                    leds.set(0, Led::Button, color, 75);
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

fn attenuate(signal: u16, level: u16) -> u16 {
    let center = 2048u32;

    // Convert to signed deviation from center
    let deviation = signal as i32 - center as i32;

    // Apply attenuation as fixed-point scaling
    let scaled = (deviation as i64 * level as i64) / 4095;

    // Add back the center and clamp to 0..=4095
    let result = center as i64 + scaled;
    result.clamp(0, 4095) as u16
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 75
}
