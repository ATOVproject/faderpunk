use defmt::info;
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use libfp::{
    constants::LED_MID,
    utils::{attenuverter, clickless, is_close, split_unsigned_value},
    Color, Config, Param, Range, Value,
};

use crate::app::{App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Quantizer", "").add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Yellow,
        Color::Purple,
        Color::Blue,
        Color::Red,
        Color::White,
    ],
});
// pub static CONFIG: Config<PARAMS> = Config::new("Envelope Follower", "audio amplitude to CV");

// const led_color.into(): RGB8 = ATOV_BLUE;
const BUTTON_BRIGHTNESS: u8 = LED_MID;

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
pub struct Params<'a> {
    color: ParamSlot<'a, Color, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new([Value::Color(Color::Yellow)], app.app_id, app.start_channel);

    let params = Params {
        color: ParamSlot::new(&param_store, 0),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let led_color = params.color.get().await;

    leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);

    let range = Range::_0_10V;
    let _input = app.make_in_jack(0, range).await;
    let output = app.make_out_jack(1, range).await;

    let oct_glob = app.make_global(4095);
    let latched_glob = app.make_global([false; 2]);
    let st_glob = app.make_global(0);

    let offset = storage.query(|s| s.offset_saved).await;
    let att = storage.query(|s| s.att_saved).await;

    st_glob.set(offset).await;
    oct_glob.set(att).await;

    leds.set(0, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);
    leds.set(1, Led::Button, led_color.into(), BUTTON_BRIGHTNESS);

    let range = Range::_0_10V;

    let mut quantizer = app.use_quantizer(range);
    let fut1 = async {
        let mut old_button = [false; 2];
        let mut oct = 0;
        let mut st = 0;

        loop {
            app.delay_millis(1).await;

            let inval = _input.get_value();
            // let inval = _input.get_value() as f32 / 410. - 0.029268293;

            oct = (((oct_glob.get().await * 10 / 4095) as f32 - 5.) * 410.) as u16;

            st = ((st_glob.get().await * 12 / 4095) as f32 * 410.) as u16;

            let outval = quantizer.get_quantized_note(inval + oct as u16 + st as u16);

            output.set_value(outval.as_counts(range));

            for n in 0..2 {
                if !old_button[n] && buttons.is_button_pressed(n) {
                    latched_glob.set([false; 2]).await;
                    old_button[n] = true;
                }
                if old_button[n] && !buttons.is_button_pressed(n) {
                    latched_glob.set([false; 2]).await;
                    old_button[n] = false;
                }
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();
            let mut latched = latched_glob.get().await;

            if chan == 0 {
                let offset = st_glob.get().await;
                if is_close((offset) as u16, vals[chan]) {
                    latched[chan] = true;
                    latched_glob.set(latched).await;
                }

                if latched[chan] {
                    st_glob.set(vals[chan]).await;
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
                let att = oct_glob.get().await;
                if is_close(att, vals[chan]) {
                    latched[chan] = true;
                    latched_glob.set(latched).await;
                }
                if latched[chan] {
                    oct_glob.set(vals[chan]).await;

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
                    st_glob.set(0).await;
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
                    oct_glob.set(4095).await;
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

                    let offset = storage.query(|s| s.offset_saved).await;
                    let att = storage.query(|s| s.att_saved).await;

                    st_glob.set(offset).await;
                    oct_glob.set(att).await;
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
