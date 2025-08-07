use defmt::info;
use embassy_futures::{
    join::{join3, join4},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    constants::{ATOV_PURPLE, CURVE_LOG, LED_HIGH, LED_LOW, LED_MAX, LED_MID},
    utils::{
        attenuate, attenuate_bipolar, clickless, is_close, slew_limiter, split_unsigned_value,
    },
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Value};
use smart_leds::colors::RED;

use crate::app::{
    App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, Range, SceneEvent, RGB8,
};

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 4;

pub static CONFIG: Config<PARAMS> = Config::new("RGB test app", "Fader set color")
    .add_param(Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    .add_param(Param::Bool { name: "Bipolar" })
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI CC",
        min: 1,
        max: 128,
    });

const LED_COLOR: RGB8 = ATOV_PURPLE;
const BUTTON_BRIGHTNESS: u8 = LED_MID;

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
        }
    }
}

impl AppStorage for Storage {}

// TODO: Make a macro to generate this.
pub struct Params<'a> {
    curve: ParamSlot<'a, Curve, PARAMS>,
    bipolar: ParamSlot<'a, bool, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    midi_cc: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // TODO: Make a macro to generate this.
    // TODO: Move Signal (when changed) to store so that we can do params.wait_for_change maybe
    // TODO: Generate this from the static params defined above
    let param_store = ParamStore::new(
        [
            Value::Curve(Curve::Linear),
            Value::bool(false),
            Value::i32(1),
            Value::i32(32),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        curve: ParamSlot::new(&param_store, 0),
        bipolar: ParamSlot::new(&param_store, 1),
        midi_channel: ParamSlot::new(&param_store, 2),
        midi_cc: ParamSlot::new(&param_store, 3),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();

    let mut color = [255; 3];
    let intensity = [LED_LOW, LED_MID, LED_HIGH, LED_MAX];
    loop {
        let chan = fader.wait_for_any_change().await;
        let val = fader.get_all_values();
        color[chan] = (val[chan] / 16) as u8;
        let rgb: smart_leds::RGB<u8> = RGB8 {
            r: color[0],
            g: color[1],
            b: color[2],
        };

        for n in 0..4 {
            leds.set(n, Led::Top, rgb, intensity[n]);
            leds.set(n, Led::Bottom, rgb, intensity[n]);
            leds.set(n, Led::Button, rgb, intensity[n]);
        }
        info!("R: {}, G: {}, B: {}", color[0], color[1], color[2])
    }
}
