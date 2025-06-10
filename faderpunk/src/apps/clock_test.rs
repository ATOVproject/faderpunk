use config::Config;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, Led};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("Clock test", "Visualize clock tempo");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let mut clock = app.use_clock();
    let color = (243, 191, 78);
    let leds = app.use_leds();
    let mut cur: usize = 0;
    loop {
        clock.wait_for_tick(1).await;
        leds.set(cur, Led::Button, color, 100);
        loop {
            let is_reset = clock.wait_for_tick(6).await;
            if is_reset {
                leds.set(cur, Led::Button, color, 0);
                cur = 0;
                break;
            } else {
                cur = (cur + 1) % 16;
                leds.set(cur, Led::Button, color, 100);
            }
            let prev = if cur == 0 { 15 } else { cur - 1 };
            leds.set(prev, Led::Button, color, 0);
        }
    }
}
