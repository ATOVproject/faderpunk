use config::Config;
use embassy_futures::{join::join, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::{
    app::{App, Led},
    storage::Store,
};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("Clock test", "Visualize clock tempo");

pub struct Params {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = Store::new([], app.app_id, app.start_channel);
    let params = Params {};

    select(
        join(run(&app, &params), param_store.param_handler()),
        app.exit_handler(exit_signal),
    )
    .await;
}

pub async fn run(app: &App<CHANNELS>, _params: &Params) {
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
