use config::Config;

use crate::app::{App, Led};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Clock test", "Visualize clock tempo");

pub async fn run(app: App<CHANNELS>) {
    let mut clock = app.use_clock();
    let color = (243, 191, 78);
    let leds = app.use_leds();
    let mut cur: usize = 0;
    loop {
        cur = (cur + 1) % 16;
        let prev = if cur == 0 { 15 } else { cur - 1 };
        clock.wait_for_tick(6).await;
        leds.set(cur, Led::Button, color, 100);
        leds.set(prev, Led::Button, color, 0);
    }
}
