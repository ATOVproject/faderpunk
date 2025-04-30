//Todo
//add a function to the button
//add LED to the button

use crate::{app::{App, Led, Range}, tasks::buttons};
use config::Config;
use embassy_futures::join::{self, join3};

pub const CHANNELS: usize = 1;

app_config!(
    config("Random CV", "clocked random CV");

    params(
    );
    storage(
    );
);

pub async fn run(app: App<'_, CHANNELS>, ctx: &AppContext<'_>) {
    
    let mut clock = app.use_clock();
    let mut rnd = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let glob_muted = app.make_global(false);


    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    
    let mut div: [u16; 1] = [24];
    
    let mut clkn = 0;

    const LED_COLOR: (u8, u8, u8) = (188, 77, 216);

    leds.set(0, Led::Button, LED_COLOR, 100);

    let fut1 = async {
        loop {
            clock.wait_for_tick(1).await;
            clkn += 1;
            let muted = glob_muted.get().await;
            div = fader.get_values();
            div[0] = (25 - (((div[0] as u32 * 24) /4095) + 1)) as u16;
            if clkn % div[0] == 0 && !muted {
                let val = rnd.roll();
                output.set_value(val);
                leds.set(0, Led::Top, LED_COLOR, (val / 16) as u8);
                leds.set(0, Led::Bottom, LED_COLOR, (255 - val / 16) as u8);
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_any_down().await;
            let muted = glob_muted.toggle().await;
            if muted {
                leds.set(0, Led::Button, LED_COLOR, 0);
                output.set_value(2047);
                leds.set(0, Led::Top, LED_COLOR, 0);
                leds.set(0, Led::Bottom, LED_COLOR, 0);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 75);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change(0).await;
        }
    };

    join3(fut1, fut2, fut3).await;


}
