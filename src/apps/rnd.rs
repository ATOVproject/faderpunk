use crate::config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::join3;
// use minicbor::encode;

use crate::app::App;

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
    info!("App default started on channel: {}", app.channels[0]);

    let config = APP_CONFIG.to_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);

    let jacks = app.make_out_jack(0).await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = glob_muted.get().await;
            if !muted {
                let vals = rand();

            
            }
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let [fader] = app.get_fader_values();
            info!("Moved fader {} to {}", app.channels[0], fader);
            //app.midi_send_cc(0, fader).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            info!("Pressed button {}", app.channels[0]);
            let muted = glob_muted.toggle().await;
            if muted {
                jacks.set_values(0);
            }
        }
    };

    join3(fut1, fut2, fut3).await;
}
