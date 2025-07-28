use config::{Config, Curve, Param, Value};
use defmt::info;
use embassy_futures::{
    join::{join3, join4},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{constants::CURVE_LOG, utils::slew_limiter};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Value};

use crate::app::{
    App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, Range, SceneEvent, RGB8,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 3;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
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
    midi_cc: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // TODO: Make a macro to generate this.
    // TODO: Move Signal (when changed) to store so that we can do params.wait_for_change maybe
    // TODO: Generate this from the static params defined above
    let param_store = ParamStore::new(
        [Value::Curve(Curve::Linear), Value::i32(1), Value::i32(32)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        curve: ParamSlot::new(&param_store, 0),
        midi_channel: ParamSlot::new(&param_store, 1),
        midi_cc: ParamSlot::new(&param_store, 2),
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

    let midi_chan = params.midi_channel.get().await;
    let midi_cc = params.midi_cc.get().await;
    let curve = params.curve.get().await;
    let midi = app.use_midi(midi_chan as u8 - 1);

    let muted_glob = app.make_global(false);

    storage.load(None).await;

    let muted = storage.query(|s| s.muted).await;
    muted_glob.set(muted).await;

    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let fut1 = async {
        let mut outval = 0.;
        let mut val = fader.get_value();
        let mut old_midi = 0;

        loop {
            app.delay_millis(1).await;
            let muted = muted_glob.get().await;
            let fadval = fader.get_value();

            if muted {
                val = 0;
            } else {
                val = curve.at(fadval.into());
            }

            outval = clickless(outval, val);

            jack.set_value(outval as u16);
            leds.set(0, Led::Top, LED_COLOR, (outval as u16 / 16) as u8);

            if old_midi != outval as u16 / 32 {
                midi.send_cc(midi_cc as u8, outval as u16).await;
                old_midi = outval as u16 / 32;
                info!("midi = {}", old_midi);
            }
        }
    };

    let fut2 = async {
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
            muted_glob.set(muted).await;
            if muted {
                leds.reset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 100);
            }
        }
    };
    let fut3 = async {
        loop {
            fader.wait_for_change().await;
            if buttons.is_shift_pressed() {
                //do attenuation setting here
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let muted = storage.query(|s| s.muted).await;
                    muted_glob.set(muted).await;
                    if muted {
                        leds.reset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, LED_COLOR, 100);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

fn clickless(prev: f32, input: u16) -> f32 {
    let delta = input as i32 - prev as i32;
    let step = 205.;
    if delta > 0 {
        if prev + step < input as f32 {
            prev + step
        } else {
            input as f32
        }
    } else if delta < 0 {
        if prev - step > input as f32 {
            prev - step
        } else {
            input as f32
        }
    } else {
        input.clamp(0, 4095) as f32
    }
}
