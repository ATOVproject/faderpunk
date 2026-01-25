use embassy_futures::{
    join::join4,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue, latch::LatchLayer, utils::split_unsigned_value, AppIcon, Brightness, Color,
    APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Quantizer",
    "Quantize CV passing through",
    Color::Blue,
    AppIcon::Quantize,
)
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
});

pub struct Params {
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self { color: Color::Blue }
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

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    oct: u16,
    st: u16,
    offset_toggles: [bool; 2],
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            oct: 2047,
            st: 0,
            offset_toggles: [false; 2],
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
    let led_color = params.query(|p| p.color);
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    leds.set(0, Led::Button, led_color, Brightness::Mid);
    leds.set(1, Led::Button, led_color, Brightness::Mid);

    let range = Range::_Neg5_5V;
    let quantizer = app.use_quantizer(range);
    let _input = app.make_in_jack(0, range).await;
    let output = app.make_out_jack(1, range).await;
    for chan in 0..2 {
        if !storage.query(|s| s.offset_toggles[chan]) {
            leds.set(chan, Led::Button, led_color, Brightness::Mid);
        } else {
            leds.unset(chan, Led::Button);
        }
    }

    let main_loop = async {
        loop {
            app.delay_millis(1).await;

            let inval = _input.get_value() as i16;

            let oct = if storage.query(|s| s.offset_toggles[1]) {
                0
            } else {
                (((storage.query(|s| s.oct) * 10 / 4095) as f32 - 5.) * 410.) as i16
            };

            let st = if storage.query(|s| s.offset_toggles[0]) {
                0
            } else {
                ((storage.query(|s| s.st) * 12 / 4095) as f32 * 410. / 12.) as i16
            };

            let outval = quantizer
                .get_quantized_note((inval + oct + st).clamp(0, 4095) as u16)
                .await;

            output.set_value(outval.as_counts(range));
            let oct_led = split_unsigned_value(outval.as_counts(range));
            leds.set(1, Led::Top, led_color, Brightness::Custom(oct_led[0]));
            leds.set(1, Led::Bottom, led_color, Brightness::Custom(oct_led[1]));
            leds.set(
                0,
                Led::Top,
                led_color,
                Brightness::Custom((st * 255 / 410) as u8),
            );
        }
    };

    let button_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if is_shift_pressed {
            } else {
                storage.modify_and_save(|s| {
                    s.offset_toggles[chan] = !s.offset_toggles[chan];
                });
                if !storage.query(|s| s.offset_toggles[chan]) {
                    leds.set(chan, Led::Button, led_color, Brightness::Mid);
                } else {
                    leds.unset(chan, Led::Button);
                }
            }
        }
    };

    let fader_event_handler = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = LatchLayer::Main;

            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.st),
                    LatchLayer::Alt => 0,
                    LatchLayer::Third => 0,
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| {
                                s.st = new_value;
                            });
                        }
                        LatchLayer::Alt => {}
                        LatchLayer::Third => {}
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.oct),
                    LatchLayer::Alt => 0,
                    LatchLayer::Third => 0,
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| {
                                s.oct = new_value;
                            });
                        }
                        LatchLayer::Alt => {}
                        LatchLayer::Third => {}
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    for chan in 0..2 {
                        if !storage.query(|s| s.offset_toggles[chan]) {
                            leds.set(chan, Led::Button, led_color, Brightness::Mid);
                        } else {
                            leds.unset(chan, Led::Button);
                        }
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join4(
        main_loop,
        button_handler,
        fader_event_handler,
        scene_handler,
    )
    .await;
}
