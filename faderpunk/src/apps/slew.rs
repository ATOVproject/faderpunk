// todo
// find what to do with the buttons


use config::Config;
use defmt::info;
use embassy_futures::{join::{join3, join4}, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};
use smart_leds::colors::RED;

use crate::app::{App, AppStorage, Led, ManagedStorage, Range, SceneEvent, RGB8};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Slew Limiter", "slow");

const LED_COLOR: RGB8 = RGB8 {
    r: 188,
    g: 77,
    b: 216,
};
const BUTTON_BRIGHTNESS: u8 = 75;

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; 2],
    att_saved: u16,
    offset_saved: i32,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2000; 2],
            att_saved: 4095,
            offset_saved: 2047,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            run(&app, storage).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
    let _input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let _output = app.make_out_jack(1, Range::_Neg5_5V).await;

    let attack_glob = app.make_global(1);
    let decay_glob = app.make_global(1);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global(false);
    let offset_glob = app.make_global(0);

    let mut oldval = 0.;
    let mut shift_old = false;

    storage.load(None).await;
    let stored_faders = storage.query(|s| s.fader_saved).await;
    let offset = storage.query(|s| s.offset_saved).await;
    let att = storage.query(|s| s.att_saved).await;

    offset_glob.set(offset).await;
    att_glob.set(att).await;
    attack_glob.set(stored_faders[0]).await;
    decay_glob.set(stored_faders[1]).await;

    leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);

    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let inval = _input.get_value();


            oldval = slew_limiter(
                oldval,
                inval,
                attack_glob.get().await,
                decay_glob.get().await,
            ).clamp(0., 4095.);

            let att = att_glob.get().await;
            let offset = offset_glob.get().await;

            let outval = ((attenuate(oldval as u16, att) as i32 + offset) as u16).clamp(0, 4095);

            _output.set_value(oldval as u16);

            if !buttons.is_shift_pressed() {
                let slew_led = split_unsigned_value(oldval as u16);
                leds.set(0, Led::Top, LED_COLOR, slew_led[0]);
                leds.set(0, Led::Bottom, LED_COLOR, slew_led[1]);

                let out_led = split_unsigned_value(outval);
                leds.set(1, Led::Top, LED_COLOR, out_led[0]);
                leds.set(1, Led::Bottom, LED_COLOR, out_led[1]);
            } else {
                let off_led = split_signed_value(offset);
                leds.set(0, Led::Top, RED, off_led[0]);
                leds.set(0, Led::Bottom, RED, off_led[1]);
                let att_led = split_unsigned_value(att);
                leds.set(1, Led::Top, RED, att_led[0]);
                leds.set(1, Led::Bottom, RED, att_led[1]);
            }

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = true;
                leds.reset(0, Led::Button);
                leds.reset(1, Led::Button);
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = false;

                leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
                leds.set(1, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            
            if !buttons.is_shift_pressed() {
                storage.load(None).await;
                let mut stored_faders = storage.query(|s| s.fader_saved).await;
                if chan == 0 {
                    attack_glob.set(vals[chan]).await;
                    stored_faders[chan] = vals[chan];
                }

                if chan == 1 {
                    decay_glob.set(vals[chan]).await;
                    stored_faders[chan] = vals[chan];  
                }

            storage
                    .modify_and_save(
                        |s| {
                            s.fader_saved = stored_faders;
                            s.fader_saved
                        },
                        None,
                    )
                    .await;

            } else {
                if chan == 0 {
                    let offset = vals[chan] as i32 - 2047;
                    offset_glob.set(offset).await;

                    storage
                    .modify_and_save(
                        |s| {
                            s.offset_saved = offset;
                            s.offset_saved
                        },
                        None,
                    )
                    .await;
                }

                if chan == 1 {
                    att_glob.set(vals[chan]).await;

                    storage
                    .modify_and_save(
                        |s: &mut Storage| {
                            s.att_saved = vals[chan];
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
            let chan = buttons.wait_for_any_down().await;
            if !buttons.is_shift_pressed() {
                if chan == 0 {}
            } else {
                if chan == 0 {
                    offset_glob.set(0).await;
                    storage
                    .modify_and_save(
                        |s| {
                            s.offset_saved = 0;
                            s.offset_saved
                        },
                        None,
                    )
                    .await;
                }
                if chan == 1 {
                    att_glob.set(4095).await;
                    storage
                    .modify_and_save(
                        |s| {
                            s.att_saved = 4095;
                            s.att_saved
                        },
                        None,
                    )
                    .await;
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    let stored_faders = storage.query(|s| s.fader_saved).await;
                    let offset = storage.query(|s| s.offset_saved).await;
                    let att = storage.query(|s| s.att_saved).await;

                    offset_glob.set(offset).await;
                    att_glob.set(att).await;
                    attack_glob.set(stored_faders[0]).await;
                    decay_glob.set(stored_faders[1]).await;
                    

                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

fn rectify(value: u16) -> u16 {
    let result = value.abs_diff(2047) + 2047;
    result
}

fn slew_limiter(prev: f32, input: u16, rise_rate: u16, fall_rate: u16) -> f32 {
    let delta = input as i32 - prev as i32;
    if delta > 0 {
        let step = (4095 - rise_rate) as f32 / 100. + 1.;
        prev + step
    } else if delta < 0 {
        let step = (4095 - fall_rate) as f32 / 100. + 1.;
        prev - step
    } else {
        input.clamp(0, 4095) as f32
    }
}

fn attenuate(input: u16, modulation: u16) -> u16 {
    let input = input as i32;
    let mod_val = modulation as i32;

    // Map modulation (0..=4095) to a blend factor from -1.0 (invert) to +1.0 (normal)
    let blend = (mod_val - 2047) as f32 / 2048.0;

    // Normal = input, Inverted = 4095 - input
    let normal = input as f32;
    let inverted = (4095 - input) as f32;

    // Interpolate between inverted and normal
    let result = inverted * (1.0 - blend) / 2.0 + normal * (1.0 + blend) / 2.0;

    result.clamp(0.0, 4095.0) as u16
}

fn split_unsigned_value(input: u16) -> [u8; 2] {
    let clamped = input.min(4095);
    if clamped <= 2047 {
        let neg = ((2047 - clamped)/8 ).min(255) as u8;
        [0, neg]
    } else {
        let pos = ((clamped - 2047)/8).min(255) as u8;
        [pos, 0]
    }
}


fn split_signed_value(input: i32) -> [u8; 2] {
    let clamped = input.clamp(-2047, 2047);
    if clamped >= 0 {
        let pos = ((clamped * 255 + 1023) / 2047) as u8;
        [pos, 0]
    } else {
        let neg = (((-clamped) * 255 + 1023) / 2047) as u8;
        [0, neg]
    }
}



    