use config::Config;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::{
    app::{App, ClockEvent, Led, RGB8},
    storage::ParamStore,
};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("Clock test", "Visualize clock tempo");

pub struct Params {}

const LED_COLOR: RGB8 = RGB8 {
    r: 243,
    g: 191,
    b: 78,
};

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new([], app.app_id, app.start_channel);
    let params = Params {};

    let app_loop = async {
        loop {
            select(run(&app, &params), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, _params: &Params) {
    let mut clock = app.use_clock();
    let leds = app.use_leds();
    let mut cur: usize = 0;
    loop {
        if let ClockEvent::Tick = clock.wait_for_event(1).await {
            leds.set(cur, Led::Button, LED_COLOR, 100);
            loop {
                match clock.wait_for_event(6).await {
                    ClockEvent::Reset => {
                        leds.set(cur, Led::Button, LED_COLOR, 0);
                        cur = 0;
                        break;
                    }
                    ClockEvent::Tick => {
                        cur = (cur + 1) % 16;
                        leds.set(cur, Led::Button, LED_COLOR, 100);
                    }
                    _ => {}
                }
                let prev = if cur == 0 { 15 } else { cur - 1 };
                leds.set(prev, Led::Button, LED_COLOR, 0);
            }
        }
    }
}
