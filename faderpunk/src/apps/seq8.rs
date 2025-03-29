use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::{join3, join4};
use embassy_sync::channel;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 8;
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

    app.set_led(0, Led::Button, LED_COLOR, 75);

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let clockn_glob = app.make_global(0);

    let cv1 = app.make_out_jack(0, Range::_0_10V).await;
    let gate1 = app.make_gate_jack(1, 4095).await;

    let mut seq_glob = app.make_global([0, 0, 0, 0, 0, 0, 0, 0]);

    let seq_init = faders.get_values();
    seq_glob.set(seq_init).await;



    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let clockn = clockn_glob.get().await;
            let seq = seq_glob.get().await;
            cv1.set_value(seq[clockn]);
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let mut seq = seq_glob.get().await;
            seq[chan] = vals[chan];
            seq_glob.set(seq).await;
            }
        
    };

    let fut3 = async {
        loop {
            let chan= buttons.wait_for_any_down().await;
            //info!("{}", chan);
        }
    };

    let fut4 = async {
        loop {
            //clk.wait_for_tick(24).await;
            app.delay_millis(100).await;
            gate1.set_high().await;
            app.delay_millis(25).await;
            gate1.set_low().await;
            let mut clockn = clockn_glob.get().await;
            clockn += 1;
            clockn = clockn % 8;
            clockn_glob.set(clockn).await;
        }
    };

    join4(fut1, fut2, fut3, fut4).await;
}
