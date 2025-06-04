use config::Config;
use embassy_futures::{join::join, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::App;

use super::temp_param_loop;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> =
    Config::new("Trigger", "Test app to test the clock and GPOs");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(join(run(&app), temp_param_loop()), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let jack = app.make_gate_jack(0, 2048).await;
    let mut clock = app.use_clock();
    // let color = (243, 191, 78);
    loop {
        // TODO: We need to implement a waiter for this somehow
        // An app can have as many clock waiters as it has channels
        clock.wait_for_tick(1).await;
        jack.set_high().await;
        // TODO: We need an app.led_blink or something, otherwise one won't be able to see the led
        // blink
        // app.set_led(0, Led::Button, color, 100);
        // TODO: We should also have a trigger function that adjusts the trigger length to the
        // clock
        app.delay_millis(5).await;
        jack.set_low().await;
        // app.set_led(0, Led::Button, color, 0);
    }
}
