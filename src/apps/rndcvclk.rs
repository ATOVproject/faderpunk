//Todo
//add a function to the button
//add LED to the button

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;

pub async fn run(mut app: App<CHANNELS>) {
    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let mut rnd = app.make_die();
    let mut div: [u16; 1] = [24];
    let color = (255, 255, 255);
    let mut clkn = 0;


    loop {
        app.wait_for_clock(24).await;
        clkn += 1;
        div = app.get_fader_values();
        div[0] = (25 - (((div[0] as u32 * 24) /4095) + 1)) as u16;
        if clkn % div[0] == 0{
            let val = rnd.roll();
            info!("div value {}", div[0]);
            output.set_value(val);
            app.set_led(0, Led::Top, color, (val / 16) as u8);
            app.set_led(0, Led::Bottom, color, (255 - val / 16) as u8);
        }
    }
}
