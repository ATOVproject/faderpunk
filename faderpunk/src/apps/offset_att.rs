use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    utils::{attenuverter, clickless, is_close, split_unsigned_value},
    Brightness, Color, Config, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new("Offset+Attenuator", "").add_param(Param::Color {
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
    offset_saved: i32,
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

    leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let output = app.make_out_jack(1, Range::_Neg5_5V).await;

    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global([false; 2]);
    let offset_glob = app.make_global(0);

    let offset = storage.query(|s| s.offset_saved);
    let att = storage.query(|s| s.att_saved);

    offset_glob.set(offset);
    att_glob.set(att);

    leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);

    let fut1 = async {
        let mut old_button = [false; 2];
        let mut att = 0;
        let mut offset = 0;

        loop {
            app.delay_millis(1).await;
            let inval = input.get_value();

            att = clickless(att, att_glob.get());
            offset = clickless(offset, (offset_glob.get() + 2047) as u16);

            let outval = ((attenuverter(inval as u16, att as u16) as i32 + (offset - 2047) as i32)
                .clamp(0, 4095) as u16)
                .clamp(0, 4095);
            // info!("{}", attack_glob.get().await);

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

            for n in 0..2 {
                if !old_button[n] && buttons.is_button_pressed(n) {
                    latched_glob.set([false; 2]);
                    old_button[n] = true;
                }
                if old_button[n] && !buttons.is_button_pressed(n) {
                    latched_glob.set([false; 2]);
                    old_button[n] = false;
                }
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            let mut latched = latched_glob.get();

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
    };

    let fut3 = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if is_shift_pressed {
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

                    let offset = storage.query(|s| s.offset_saved);
                    let att = storage.query(|s| s.att_saved);

                    offset_glob.set(offset);
                    att_glob.set(att);
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
