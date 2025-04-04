use config::{Config, Curve, Param};
use embassy_futures::join::join3;
use midi2::ux::u4;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("LFO", "Wooooosh");

pub async fn run(mut app: App<CHANNELS>) {

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    //let midi = app.use_midi(10);
    let mut clock = app.use_clock();
    let mut die = app.use_die();

    let jack = app.make_gate_jack(0, 2048).await;
    

    let glob_muted = app.make_global(false);
    let fad_init = faders.get_values();
    let fad_glob = app.make_global(fad_init);
    leds.set(0, Led::Button, LED_COLOR, 75);



    const LED_COLOR: (u8, u8, u8) = (78, 243, 243);

    let fut1 = async {
        loop {
            clock.wait_for_tick(1).await;
            let val = fad_glob.get().await;
            let rndval = die.roll();
            if val[0] >= rndval && !glob_muted.get().await {
                jack.set_high().await;
                leds.set(0, Led::Top, LED_COLOR, 75);
                //midi.send_note_on(75 ,4095);
                app.delay_millis(20).await;
                jack.set_low().await;
                leds.set(0, Led::Top, LED_COLOR, 0);
                //midi.send_note_off(75 as u8); 
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                leds.set(0, Led::Button, LED_COLOR, 40);
                leds.set(0, Led::Top, LED_COLOR, 0);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 75);
            }
        }
    };

    let fut3 = async {
        loop {
            faders.wait_for_change(0).await;
            let val = faders.get_values();
            fad_glob.set(val).await;
        }
    };

    join3(fut1, fut2, fut3).await;


}
