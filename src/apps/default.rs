use crate::config::{Config, Curve, Param};
use embassy_futures::join::join3;
// use minicbor::encode;

use crate::app::{App, Range, Led};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub static APP_CONFIG: Config<1> = Config::default().add_param(Param::Curve {
    name: "Curve",
    default: Curve::Linear,
    variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
});

// let mut buffer = [0u8; 128];
// let x = encode(APP_CONFIG.params(), buffer.as_mut()).unwrap();

pub async fn run(app: App<CHANNELS>) {
    let config = APP_CONFIG.to_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);

    let color: (u8, u8, u8)=  (255, 255, 100);

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
                app.set_led(0, Led::Button, color, 200);
                app.set_led(0, Led::Top, color, (fader as u16 / 16) as u8);
                app.set_led(0, Led::Bottom, color, (255 - (fader as u16) / 16) as u8);
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
                app.midi_send_cc(0, 0).await;
                app.set_led(0, Led::Button, color, 50);
                app.set_led(0, Led::Top, color, 0);
                app.set_led(0, Led::Bottom, color, 0);
            }
            else {
                let [fader] = app.get_fader_values();
                app.midi_send_cc(0, fader).await;
                app.set_led(0, Led::Button, color, 200);
                app.set_led(0, Led::Top, color, (fader as u16 / 16) as u8);
                app.set_led(0, Led::Bottom, color, (255 - (fader as u16) / 16) as u8);
            }
        
        }
    };

    join3(fut1, fut2, fut3).await;
}
