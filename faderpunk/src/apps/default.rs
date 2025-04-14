use config::{Config, Curve, Param};
use embassy_futures::join::join3;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<0> = Config::new("Default", "16n vibes plus mute buttons");

config_params!(
    PARAM_CURVE => (0, Curve, Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    }, Curve::Linear),
    PARAM_MIDI_CHANNEL => (1, u8, Param::Int {
        name: "Midi channel",
        min: 0,
        max: 15,
    }, 0),
);
//
// static STORAGE_SLOT_0: StorageSlot<Curve> = StorageSlot::new(
//     0,
//     Param::Curve {
//         name: "Curve",
//         variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
//     },
//     Curve::Linear,
// );
// static STORAGE_SLOT_1: StorageSlot<i32> = StorageSlot::new(
//     1,
//     Param::Int {
//         name: "Midi channel",
//         min: 0,
//         max: 15,
//     },
//     0,
// );
//
// // FIXME:
// //
// // Maybe have something like
// // ```rust
// // config_params!(
// //   PARAM_CURVE => (Curve, Param::Curve { ... }, Curve::Linear)
// // )
// // ```
//
// pub async fn storage_listener(app_id: u8, start_channel: usize) {
//     let mut subscriber = APP_STORAGE_WATCHES[start_channel].receiver().unwrap();
//     loop {
//         if let StorageEvent::Read(storage_app_id, storage_slot, res) = subscriber.changed().await {
//             if app_id != storage_app_id {
//                 continue;
//             }
//             match storage_slot {
//                 0 => STORAGE_SLOT_0.des(&res).await,
//                 1 => STORAGE_SLOT_1.des(&res).await,
//                 _ => {}
//             }
//         }
//     }
// }
//
// pub fn get_params() -> Vec<Param, 8> {
//     let mut params = Vec::new();
//     params.push(STORAGE_SLOT_0.param);
//     params.push(STORAGE_SLOT_1.param);
//     params
// }
//
// pub async fn get_all_storage_values() -> (Curve, i32) {
//     (
//         *STORAGE_SLOT_0.inner.lock().await,
//         *STORAGE_SLOT_1.inner.lock().await,
//     )
// }
//
// pub async fn serialize_app_state_to_slice<'buf>(
//     buffer: &'buf mut [u8],
// ) -> Result<(), PostcardError> {
//     let current_state_tuple = get_all_storage_values().await;
//     to_slice(&current_state_tuple, buffer);
//     Ok(())
// }

// FIXME: START HERE!!!!!
// Start with implementing the get_app_storage_state and serialization functions for this case to
// see if it works
//
// FIXME: Make sure we rename STORAGE_SLOTs to something PARAM_0 or something like that
// FIXME: We can then do things like let curve = PARAM_0.as_global();

// FIXME: We can then do things like
// app.make_global_with_store(STORAGE_SLOT_0, false);
// Actually we don't really need to specify the storage slots like that
// Just make sure GlobalWithStorage uses StorageSlots internally

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

pub async fn run(app: App<CHANNELS>) {
    let curve = Curve::Linear;
    let midi_channel = 0;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(midi_channel);

    let glob_muted = app.make_global(false);

    let muted = glob_muted.get().await;
    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = glob_muted.get().await;
            if !muted {
                let vals = faders.get_values();
                jack.set_value_with_curve(curve, vals[0]);
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let muted = glob_muted.get().await;
            if !muted {
                let [fader] = faders.get_values();
                midi.send_cc(32 + app.start_channel as u8, fader).await;
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let muted = glob_muted.toggle().await;
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
        }
    };

    join3(fut1, fut2, fut3).await;
}
