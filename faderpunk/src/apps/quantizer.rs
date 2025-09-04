use defmt::info;
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate_bipolar, clickless, is_close, split_unsigned_value},
    Brightness, Color, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Quantizer", "Quantize CV passing through")
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

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    oct: u16,
    st: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self { oct: 2047, st: 0 }
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
    let led_color = params.query(|p| (p.color));
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    leds.set(0, Led::Button, led_color, Brightness::Lower);
    leds.set(1, Led::Button, led_color, Brightness::Lower);

    let oct_glob = app.make_global(4095);
    let latched_glob = app.make_global([false; 2]);
    let st_glob = app.make_global(0);
    let latch_layer_glob = app.make_global(LatchLayer::Main);

    let offset = storage.query(|s| s.oct);
    let att = storage.query(|s| s.st);

    st_glob.set(offset);
    oct_glob.set(att);

    let range = Range::_Neg5_5V;
    let quantizer = app.use_quantizer(range);
    let _input = app.make_in_jack(0, range).await;
    let output = app.make_out_jack(1, range).await;

    let main_loop = async {
        let mut oct = 0;
        let mut st = 0;

        loop {
            app.delay_millis(1).await;

            let inval = _input.get_value() as i16;

            oct = (((storage.query(|s| s.oct) * 10 / 4095) as f32 - 5.) * 410.) as i16;

            st = ((storage.query(|s| s.st) * 12 / 4095) as f32 * 410. / 12.) as i16;

            let outval = quantizer
                .get_quantized_note((inval + oct + st).clamp(0, 4095) as u16)
                .await;

            // info!(
            //     "inval = {}, oct = {}, st = {}, outval = {}",
            //     inval,
            //     oct,
            //     st,
            //     outval.as_counts(range)
            // );
            output.set_value(outval.as_counts(range));
            let oct_led = split_unsigned_value(outval.as_counts(range));
            leds.set(0, Led::Top, led_color, Brightness::Custom(oct_led[0]));
            leds.set(0, Led::Bottom, led_color, Brightness::Custom(oct_led[1]));
            leds.set(
                1,
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
                if chan == 0 {
                    st_glob.set(0);
                    storage
                        .modify_and_save(
                            |s| {
                                s.st = 0;
                            },
                            None,
                        )
                        .await;
                }
                if chan == 1 {
                    oct_glob.set(0);
                    storage
                        .modify_and_save(
                            |s| {
                                s.oct = 0;
                            },
                            None,
                        )
                        .await;
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
                    LatchLayer::Main => storage.query(|s| s.oct),
                    LatchLayer::Alt => 0,
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
                                        s.oct = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {}
                        _ => unreachable!(),
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.oct),
                    LatchLayer::Alt => 0,
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
                                        s.st = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {}
                        _ => unreachable!(),
                    }
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

    join4(
        main_loop,
        button_handler,
        fader_event_handler,
        scene_handler,
    )
    .await;
}
