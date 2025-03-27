use config::{Config, Curve, Param};
use embassy_futures::join::join3;
// use minicbor::encode;

use crate::app::{App, Range};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        default: Curve::Linear,
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    });

// let mut buffer = [0u8; 128];
// let x = encode(APP_CONFIG.params(), buffer.as_mut()).unwrap();

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = glob_muted.get().await;
            if !muted {
                let vals = app.get_fader_values();
                jack.set_value_with_curve(curve, vals[0]);
            }
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let muted = glob_muted.get().await;
            if !muted {
                let [fader] = app.get_fader_values();
                app.midi_send_cc(0, fader).await;
            }
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                jack.set_value(0);
            }
            waiter.debounce_button().await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
