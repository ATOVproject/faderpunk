use config::{Curve, Param};
use embassy_futures::join::join3;

use crate::{
    app::{App, Led, Range, StorageSlot},
    storage::ParamStore,
};

pub const CHANNELS: usize = 1;

app_params! (
    config("Default App", "Description");

    params(
        curve => (Curve, Curve::Linear, Param::Curve {
            name: "Curve",
            variants: &[Curve::Linear, Curve::Exponential],
        }),
        midi_channel => (i32, 0, Param::i32 {
            name: "MIDI Channel",
            min: 0,
            max: 15,
        }),
    )
);

pub async fn msg_loop(vals: &ParamStore<PARAMS>) {}

// IDEA:
// The message loop could live in the register_apps macro

// NEXT:
// - Make AppParams or Mutex values storage slots somehow. We still need to be able to get the
// Params out
// - The message loop also needs to be generated in the macro

// IDEA: !!! SCENES !!!
// - For Scenes: storage slots manage scenes
// - They need to be aware of the current scene (Just use an ATOMIC u8)
// - SCENE will be stored in eeprom as well every time it changes, to recover it after reboot

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

pub async fn run(app: App<CHANNELS>, params: AppParams<'_>) {
    let param_curve = params.curve();
    let midi_channel = params.midi_channel().get().await;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let midi = app.use_midi(midi_channel as u8);

    let mut glob_muted = app.make_global_with_store(false, StorageSlot::A);
    glob_muted.load().await;

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
            let curve = param_curve.get().await;
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
            glob_muted.save().await;
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
