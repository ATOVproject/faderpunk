use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuverter, slew_limiter, split_signed_value, split_unsigned_value},
    AppIcon, Brightness, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Slew Limiter",
    "slows CV changes",
    Color::Yellow,
    AppIcon::ArrowCircle,
)
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Yellow,
        Color::Pink,
        Color::Cyan,
        Color::Red,
        Color::White,
    ],
});

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
    offset_saved: u16,
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

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let mut oldval = 0.;
    let mut shift_old = false;

    leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color, BUTTON_BRIGHTNESS);

    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let mut inval = input.get_value();
            // inval = rectify(inval);

            oldval = slew_limiter(
                oldval,
                inval,
                storage.query(|s| (s.fader_saved[0])),
                storage.query(|s| (s.fader_saved[1])),
            )
            .clamp(0., 4095.);

            let att = storage.query(|s| (s.att_saved));
            let offset = storage.query(|s| (s.offset_saved)) as i32 - 2047;

            let outval = ((attenuverter(oldval as u16, att) as i32 + offset) as u16).clamp(0, 4095);

            output.set_value(outval);

            if latch_active_layer == LatchLayer::Main {
                let slew_led = split_unsigned_value(oldval as u16);
                leds.set(0, Led::Top, led_color, Brightness::Custom(slew_led[0]));
                leds.set(0, Led::Bottom, led_color, Brightness::Custom(slew_led[1]));

                let out_led = split_unsigned_value(outval);
                leds.set(1, Led::Top, led_color, Brightness::Custom(out_led[0]));
                leds.set(1, Led::Bottom, led_color, Brightness::Custom(out_led[1]));
                leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
                leds.set(1, Led::Button, led_color, BUTTON_BRIGHTNESS);
            } else {
                let off_led = split_signed_value(offset);
                leds.set(0, Led::Top, Color::Red, Brightness::Custom(off_led[0]));
                leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(off_led[1]));
                let att_led = split_unsigned_value(att);
                leds.set(1, Led::Top, Color::Red, Brightness::Custom(att_led[0]));
                leds.set(1, Led::Bottom, Color::Red, Brightness::Custom(att_led[1]));
                if storage.query(|s| (s.offset_saved)) == 2047 {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, Color::Red, BUTTON_BRIGHTNESS);
                }
                if storage.query(|s| (s.att_saved)) == 4095 {
                    leds.unset(1, Led::Button);
                } else {
                    leds.set(1, Led::Button, Color::Red, BUTTON_BRIGHTNESS);
                }
            }
        }
    };

    let fut2 = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.offset_saved),
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.offset_saved = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        _ => unreachable!(),
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.att_saved),
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.att_saved = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    };

    let fut3 = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if !is_shift_pressed {
            } else {
                if chan == 0 {
                    storage
                        .modify_and_save(
                            |s| {
                                s.offset_saved = 2047;
                                s.offset_saved
                            },
                            None,
                        )
                        .await;
                }
                if chan == 1 {
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
