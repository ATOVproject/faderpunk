use config::{Config, Curve, Param};
use embassy_futures::join::join3;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        default: Curve::Linear,
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    });

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);
    app.set_led(0, Led::Button, LED_COLOR, 75);

    let buttons = app.use_buttons();
    let faders = app.use_faders();

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
                app.midi_send_cc(0, fader).await;
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                app.set_led(0, Led::Button, LED_COLOR, 0);
                jack.set_value(0);
            } else {
                app.set_led(0, Led::Button, LED_COLOR, 75);
            }
            // button.debounce().await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
