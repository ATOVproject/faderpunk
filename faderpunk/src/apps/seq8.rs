use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::{join3, join4, join5};
use embassy_sync::channel;
use embassy_time::Duration;

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

    
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let mut clk = app.use_clock();


    let clockn_glob = app.make_global(0);
    let gatet = 20;

    let cv1 = app.make_out_jack(0, Range::_0_10V).await;
    let gate1 = app.make_gate_jack(1, 4095).await;

    let mut seq_glob = app.make_global([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut gateseq_glob = app.make_global([true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true]);
    
    let mut latched_glob = app.make_global([true, true, true, true, true, true, true, true]);

    //let seq_init = faders.get_values();
    //seq_glob.set(seq_init).await;



    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let clockn = clockn_glob.get().await;
            let seq = seq_glob.get().await;
            cv1.set_value(seq[clockn] / 5);
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let mut latched = latched_glob.get().await;
            let mut seq = seq_glob.get().await;
            if (vals[chan] as i16 - seq[chan] as i16).abs() < 100 && !latched[chan] {
                latched[chan] = true;  
                latched_glob.set(latched).await;              
            }
            if latched[chan] {
                seq[chan] = vals[chan];
                seq_glob.set(seq).await;
            }
        }
        
    };

    let fut3 = async {
        loop {
            let chan= buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get().await;
            gateseq[chan] = !gateseq[chan];
            gateseq_glob.set(gateseq).await;
            if gateseq[chan] {
                app.set_led(chan, Led::Button , LED_COLOR, 75);
            }
            if !gateseq[chan] {
                app.set_led(chan, Led::Button , LED_COLOR, 0);
            }
        }
    };

    let fut4 = async {
        loop {
           buttons.wait_for_long_press(0, Duration::from_secs(1)).await;
           info!("long!");

        }
    };


    let fut5 = async {
        loop {
            let gateseq = gateseq_glob.get().await;
            clk.wait_for_tick(24).await;
            let mut clockn = clockn_glob.get().await;
            clockn += 1;
            clockn = clockn % 8;
            clockn_glob.set(clockn).await;
            app.set_led(clockn, Led::Button , (255, 255, 255), 100);

            if gateseq[clockn]{
                gate1.set_high().await;
                app.delay_millis(gatet).await;
                gate1.set_low().await;
                app.set_led(clockn, Led::Button , LED_COLOR, 75);
            }

            
            if !gateseq[clockn] {
                app.delay_millis(gatet).await;
                app.set_led(clockn, Led::Button , LED_COLOR, 0);
            }
            
            }
    };

    join5(fut1, fut2, fut3, fut4, fut5).await;
}
