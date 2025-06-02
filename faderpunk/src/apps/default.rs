use config::{Curve, Param};
use embassy_futures::join::{join, join3, join4};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use serde::{Deserialize, Serialize};

use crate::app::{App, Arr, Led, Range, SceneEvent};

pub const CHANNELS: usize = 1;

// app_config! (
//     config("Default", "16n vibes plus mute buttons");
//
//     params(
//         curve => (Curve, Curve::Linear, Param::Curve {
//             name: "Curve",
//             variants: &[Curve::Linear, Curve::Exponential],
//         }),
//         midi_channel => (i32, 0, Param::i32 {
//             name: "MIDI Channel",
//             min: 0,
//             max: 15,
//         }),
//     );
//
//     storage(
//         muted => (bool, false),
//     );
// );

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

// FIXME: Make a macro to generate this. (Also create a "new" function)
#[derive(Serialize, Deserialize, Default)]
pub struct Storage {
    muted: bool,
}

#[embassy_executor::task(pool_size = 16)]
pub async fn run(app: App<CHANNELS>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    // FIXME: Make param
    let midi = app.use_midi(1);

    // FIXME: Maybe create a macro to generate this? We actually need to be able to supply default
    // values
    let storage: Mutex<NoopRawMutex, Storage> = Mutex::new(
        app.load::<Storage>(None)
            .await
            .unwrap_or(Storage::default()),
    );

    // FIXME: Definitely improve this API
    let muted = {
        let stor = storage.lock().await;
        stor.muted
    };

    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let update_outputs = async |muted: bool| {
        if muted {
            leds.set(0, Led::Button, LED_COLOR, 0);
            jack.set_value(0);
            midi.send_cc(32 + app.start_channel as u8, 0).await;
            leds.set(0, Led::Top, LED_COLOR, 0);
            leds.set(0, Led::Bottom, LED_COLOR, 0);
        } else {
            leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
            let vals = faders.get_values();
            midi.send_cc(32 + app.start_channel as u8, vals[0]).await
        }
    };

    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = {
                let stor = storage.lock().await;
                stor.muted
            };
            // let curve = param_curve.get().await;
            if !muted {
                let vals = faders.get_values();
                jack.set_value_with_curve(Curve::Linear, vals[0]);
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            // FIXME: Definitely improve this API
            let muted = {
                let stor = storage.lock().await;
                stor.muted
            };
            if !muted {
                let [fader] = faders.get_values();
                midi.send_cc(32 + app.start_channel as u8, fader).await;
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            // FIXME: Definitely improve this API (maybe closure?)
            let mut stor = storage.lock().await;
            stor.muted = !stor.muted;
            app.save(&*stor, None).await;
            update_outputs(stor.muted).await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    defmt::info!("LOADING SCENE {}", scene);
                    let mut stor = storage.lock().await;
                    let scene_stor = app
                        .load::<Storage>(Some(scene))
                        .await
                        .unwrap_or(Storage::default());
                    *stor = scene_stor;
                    update_outputs(stor.muted).await;
                }
                SceneEvent::SaveScene(scene) => {
                    defmt::info!("SAVING SCENE {}", scene);
                    let stor = storage.lock().await;
                    app.save(&*stor, Some(scene)).await;
                }
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
