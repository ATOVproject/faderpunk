use config::{Config, Curve, Param, Value};
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, Range, SceneEvent, RGB8,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

pub static CONFIG: config::Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 0,
        max: 15,
    });

const LED_COLOR: RGB8 = RGB8 {
    r: 0,
    g: 200,
    b: 150,
};
const BUTTON_BRIGHTNESS: u8 = 75;

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize, Default)]
pub struct Storage {
    muted: bool,
}

impl AppStorage for Storage {}

// TODO: Make a macro to generate this.
pub struct Params<'a> {
    curve: ParamSlot<'a, Curve, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // TODO: Make a macro to generate this.
    // TODO: Move Signal (when changed) to store so that we can do params.wait_for_change maybe
    // TODO: Generate this from the static params defined above
    let param_store = ParamStore::new(
        [Value::Curve(Curve::Linear), Value::i32(1)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        curve: ParamSlot::new(&param_store, 0),
        midi_channel: ParamSlot::new(&param_store, 1),
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
    let faders = app.use_faders();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let midi = app.use_midi(midi_chan as u8);
    storage.load(None).await;

    let muted = storage.query(|s| s.muted).await;

    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let update_outputs = async |muted: bool| {
        if muted {
            jack.set_value(0);
            midi.send_cc(32 + app.start_channel as u8, 0).await;
            leds.reset_all();
        } else {
            leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
            let vals = faders.get_values();
            midi.send_cc(32 + app.start_channel as u8, vals[0]).await
        }
    };

    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = storage.query(|s| s.muted).await;
            let curve = params.curve.get().await;
            if !muted {
                let vals = faders.get_values();
                jack.set_value(curve.at(vals[0].into()));
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let muted = storage.query(|s| s.muted).await;
            if !muted {
                let [fader] = faders.get_values();
                midi.send_cc(32 + app.start_channel as u8, fader).await;
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let muted = storage
                .modify_and_save(
                    |s| {
                        s.muted = !s.muted;
                        s.muted
                    },
                    None,
                )
                .await;
            update_outputs(muted).await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let muted = storage.query(|s| s.muted).await;
                    update_outputs(muted).await;
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
