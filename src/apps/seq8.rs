use crate::{config::{Config, Curve, Param}, tasks::clock};
use defmt::info;
use embassy_futures::join::join3;
// use minicbor::encode;

use crate::app::{App, Range, Led};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 8;

pub static APP_CONFIG: Config<1> = Config::default().add_param(Param::Curve {
    name: "Curve",
    default: Curve::Linear,
    variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
});

// let mut buffer = [0u8; 128];
// let x = encode(APP_CONFIG.params(), buffer.as_mut()).unwrap();



pub async fn run(app: App<CHANNELS>) {
    info!("SEQ8 starting");
    let config = APP_CONFIG.to_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let mut clk = 0;

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
        }
    };

     
    let fut3 = async {
        loop {
            app.wait_for_clock(24).await;
            info!("clock");
        }
    };

    join3(fut1, fut2, fut3).await;
}
