// todo
// find what to do with the buttons

use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    colors::RED,
    ext::FromValue,
    utils::{attenuverter, is_close, slew_limiter, split_signed_value, split_unsigned_value},
    Brightness, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Envelope Follower", "Audio amplitude to CV")
    .add_param(Param::Color {
        name: "Color",
        variants: &[
            Color::Yellow,
            Color::Purple,
            Color::Teal,
            Color::Red,
            Color::White,
        ],
    });

const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

pub struct Params {
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            color: Color::Yellow,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            color: Color::from_value(values[0]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

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
            offset_saved: 0,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.start_channel);

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(
                run(&app, &param_store, storage),
                param_store.param_handler(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: ManagedStorage<Storage>,
) {
    let curve = Curve::Exponential;
    let led_color = params.query(|p| p.color);

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let output = app.make_out_jack(1, Range::_Neg5_5V).await;

    let attack_glob = app.make_global(1);
    let decay_glob = app.make_global(1);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global([false; 2]);
    let offset_glob = app.make_global(0);

    let mut oldval = 0.;
    let mut shift_old = false;

    let (stored_faders, offset, att) =
        storage.query(|s| (s.fader_saved, s.offset_saved, s.att_saved));

    offset_glob.set(offset);
    att_glob.set(att);
    attack_glob.set(curve.at(stored_faders[0]));
    decay_glob.set(curve.at(stored_faders[1]));

    leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);

    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let mut inval = input.get_value();
            inval = rectify(inval);

            oldval =
                slew_limiter(oldval, inval, attack_glob.get(), decay_glob.get()).clamp(0., 4095.);

            let att = att_glob.get();
            let offset = offset_glob.get();

            let outval = ((attenuverter(oldval as u16, att) as i32 + offset) as u16).clamp(0, 4095);

            output.set_value(outval);

            if !buttons.is_shift_pressed() {
                let slew_led = split_unsigned_value(oldval as u16);
                leds.set(
                    0,
                    Led::Top,
                    led_color.into(),
                    Brightness::Custom(slew_led[0]),
                );
                leds.set(
                    0,
                    Led::Bottom,
                    led_color.into(),
                    Brightness::Custom(slew_led[1]),
                );

                let out_led = split_unsigned_value(outval);
                leds.set(
                    1,
                    Led::Top,
                    led_color.into(),
                    Brightness::Custom(out_led[0]),
                );
                leds.set(
                    1,
                    Led::Bottom,
                    led_color.into(),
                    Brightness::Custom(out_led[1]),
                );
            } else {
                let off_led = split_signed_value(offset);
                leds.set(0, Led::Top, RED, Brightness::Custom(off_led[0]));
                leds.set(0, Led::Bottom, RED, Brightness::Custom(off_led[1]));
                let att_led = split_unsigned_value(att);
                leds.set(1, Led::Top, RED, Brightness::Custom(att_led[0]));
                leds.set(1, Led::Bottom, RED, Brightness::Custom(att_led[1]));
            }

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set([false; 2]);
                shift_old = true;
                leds.unset(0, Led::Button);
                leds.unset(1, Led::Button);
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set([false; 2]);
                shift_old = false;

                leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
                leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            let mut latched = latched_glob.get();

            if !buttons.is_shift_pressed() {
                let mut stored_faders = storage.query(|s| s.fader_saved);
                if is_close(stored_faders[chan], vals[chan]) {
                    latched[chan] = true;
                    latched_glob.set(latched);
                }
                if latched[chan] {
                    if chan == 0 {
                        attack_glob.set(curve.at(vals[chan]));
                        stored_faders[chan] = vals[chan];
                    }

                    if chan == 1 {
                        decay_glob.set(curve.at(vals[chan]));
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
                }
            } else {
                if chan == 0 {
                    let offset = offset_glob.get();
                    if is_close((offset + 2047) as u16, vals[chan]) {
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }

                    if latched[chan] {
                        offset_glob.set(vals[chan] as i32 - 2047);
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
                }

                if chan == 1 {
                    let att = att_glob.get();
                    if is_close(att, vals[chan]) {
                        latched[chan] = true;
                        latched_glob.set(latched);
                    }
                    if latched[chan] {
                        att_glob.set(vals[chan]);

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
        }
    };

    let fut3 = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if !is_shift_pressed {
                // if chan == 0 {
                //     leds.reset(0, Led::Button);
                //     attack_glob.set(0);
                //     storage
                //         .modify_and_save(
                //             |s| {
                //                 s.fader_saved[chan] = 0;
                //                 s.fader_saved
                //             },
                //             None,
                //         )
                //         .await;
                // }
                // if chan == 1 {
                //     leds.reset(1, Led::Button);
                //     decay_glob.set(0);
                //     storage
                //         .modify_and_save(
                //             |s| {
                //                 s.fader_saved[chan] = 0;
                //                 s.fader_saved
                //             },
                //             None,
                //         )
                //         .await;
                // }
            } else {
                if chan == 0 {
                    offset_glob.set(0);
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
                    att_glob.set(4095);
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

                    let (stored_faders, offset, att) =
                        storage.query(|s| (s.fader_saved, s.offset_saved, s.att_saved));

                    offset_glob.set(offset);
                    att_glob.set(att);
                    attack_glob.set(curve.at(stored_faders[0]));
                    decay_glob.set(curve.at(stored_faders[1]));
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

fn rectify(value: u16) -> u16 {
    value.abs_diff(2047) + 2047
}
