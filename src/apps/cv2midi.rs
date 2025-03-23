use crate::config::{Config, Curve, Param};
use embassy_futures::join::{self, join};
// use minicbor::encode;

use crate::app::{App, Range, Led};


pub const CHANNELS: usize = 1;

pub static APP_CONFIG: Config<1> = Config::default().add_param(Param::Curve {
    name: "Curve",
    default: Curve::Linear,
    variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
});
//Functions:
    //CV input, Fader attenuate, Button mutes


//TODO: 
//Shift + fader = change of input range
//latching for fader.
//when unmute reapply LEDs

pub async fn run(app: App<CHANNELS>) {
    let config = APP_CONFIG.to_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);

    let mut midivalold = 0;
    let color: (u8, u8, u8)=  (255, 0, 0);

    let jack = app.make_in_jack(0, Range::_Neg5_5V).await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let mut midival = jack.get_value() as u32;
            let [fader] = app.get_fader_values();
            midival = midival * fader as u32 /4096;
            let muted = glob_muted.get().await;
            if midival / 16 != midivalold / 16 && !muted {
                app.midi_send_cc(0, midival as u16).await; 
                midivalold = midival;
                app.set_led(0, Led::Button, color, 200);
                app.set_led(0, Led::Top, color, (midival as f32 / 16.0) as u8);
                app.set_led(0, Led::Bottom, color, (255.0 - (midival as f32) / 16.0) as u8);
            }
            

        }
    };


    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                app.midi_send_cc(0, 0).await;
                app.set_led(0, Led::Button, color, 50);
                app.set_led(0, Led::Top, color, 0);
                app.set_led(0, Led::Bottom, color, 0);
            }
            else {
                let mut midival = jack.get_value() as u32;
                let [fader] = app.get_fader_values();
                midival = midival * fader as u32 /4096;
                app.midi_send_cc(0, midival as u16).await; 
                app.set_led(0, Led::Button, color, 200);
                app.set_led(0, Led::Top, color, (midival as f32 / 16.0) as u8);
                app.set_led(0, Led::Bottom, color, (255.0 - (midival as f32) / 16.0) as u8);

            }

        }
    };


    join(fut1, fut2).await;
}
