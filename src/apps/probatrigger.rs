use embassy_rp::rom_data::bootrom_state_reset;

use crate::app::{App, Led};

pub const CHANNELS: usize = 1;

pub async fn run(mut app: App<CHANNELS>) {
    let jack = app.make_gate_jack(0, 2048).await;
    let mut rnd = app.make_die();
    let mut muted: bool = false;
    let color = (243, 191, 78);
    let mut b_state: bool = false;
    loop {
        // TODO: We need to implement a waiter for this somehow
        // An app can have as many clock waiters as it has channels
        app.wait_for_clock(24).await;
        let val = app.get_fader_values();
        let rndval = rnd.roll();
        if val[0] >= rndval && !muted {
            jack.set_high().await;
            app.set_led(0, Led::Top, color, 100);
            app.delay_millis(20).await;
            jack.set_low().await;
            app.set_led(0, Led::Top, color, 0);
        }
        let button = app.is_button_pressed(0);
        if button && !b_state {
            b_state = true;
            muted = !muted;
        }
        if !button && b_state {
            b_state = false;
        }
        if muted{
            app.set_led(0, Led::Button, color, 50)  
        }
        if !muted{
            app.set_led(0, Led::Button, color, 75)  
        }
        
        // app.set_led(0, Led::Button, color, 0);
    }
}
