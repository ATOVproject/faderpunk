use config::{Curve, Param};
use defmt::info;
use embassy_futures::{
    join::join3,
    select::{select, Either},
};

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;

app_config! (
    config("Default", "16n vibes plus mute buttons");

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
    );

    storage(
        muted => (bool, false),
    );
);

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

pub async fn run(app: App<'_, CHANNELS>, ctx: &AppContext<'_>) {
    let param_curve = &ctx.params.curve;
    let param_midi_channel = &ctx.params.midi_channel;
    let stor_muted = &ctx.storage.muted;

    let midi_channel = param_midi_channel.get().await;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(midi_channel as u8);

    let muted = stor_muted.get().await;
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
            // let muted = stor_muted.get().await;
            // let curve = param_curve.get().await;
            // if !muted {
            //     let vals = faders.get_values();
            //     jack.set_value_with_curve(curve, vals[0]);
            // }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let muted = stor_muted.get().await;
            if !muted {
                let [fader] = faders.get_values();
                midi.send_cc(32 + app.start_channel as u8, fader).await;
            }
        }
    };

    let fut3 = async {
        loop {
            if let Either::First(_) =
                select(buttons.wait_for_down(0), app.wait_for_scene_change()).await
            {
                stor_muted.toggle().await;
                stor_muted.save().await;
            }
            let muted = stor_muted.get().await;
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
