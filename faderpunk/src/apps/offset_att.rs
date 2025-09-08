use defmt::info;
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuverter, clickless, is_close, split_unsigned_value},
    Brightness, Color, Config, Param, Range, Value, APP_MAX_PARAMS,
};
use smart_leds::brightness;

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new("Offset+Attenuverter", "Offset and attenuvert CV")
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
    att_saved: u16,
    offset_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
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
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let led_color = params.query(|p| p.color);

    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let output = app.make_out_jack(1, Range::_Neg5_5V).await;

    let fut1 = async {
        let mut att = 0;
        let mut offset_fad = 0;
        loop {
            app.delay_millis(1).await;
            let inval = input.get_value();

            att = clickless(att, storage.query(|s| (s.att_saved)));
            offset_fad = clickless(offset_fad, storage.query(|s| (s.offset_saved)));
            let offset = offset_fad as i32 - 2047;

            let mut outval =
                (attenuverter(inval as u16, att as u16) as i32 + offset).clamp(0, 4095) as u16;
            outval = ((outval as i32 - 2047) * 2 + 2047).clamp(0, 4094) as u16;

            output.set_value(outval as u16);

            let slew_led = split_unsigned_value(inval as u16);
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
            if storage.query(|s| (s.offset_saved)) != 2047 {
                leds.set(0, Led::Button, led_color.into(), Brightness::Low);
            } else {
                leds.set(0, Led::Button, led_color.into(), Brightness::Lower);
            }
            if storage.query(|s| (s.att_saved)) != 3071 {
                leds.set(1, Led::Button, led_color.into(), Brightness::Low);
            } else {
                leds.set(1, Led::Button, led_color.into(), Brightness::Lower);
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

            if chan == 0 {
                if let Some(new_value) = latch[chan].update(
                    faders.get_value_at(chan),
                    LatchLayer::Main,
                    storage.query(|s| (s.offset_saved)),
                ) {
                    storage
                        .modify_and_save(
                            |s| {
                                s.offset_saved = new_value;
                            },
                            None,
                        )
                        .await;
                }
            }

            if chan == 1 {
                let target_value = storage.query(|s| s.att_saved);

                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), LatchLayer::Main, target_value)
                {
                    storage
                        .modify_and_save(
                            |s| {
                                s.att_saved = new_value;
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
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if is_shift_pressed {
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
                                s.att_saved = 3071;
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
