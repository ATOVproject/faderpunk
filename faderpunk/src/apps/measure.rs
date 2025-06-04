use defmt::info;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::App;
use config::Config;

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> =
    Config::new("Measure", "Test app to measure port voltages");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let mut die = app.use_die();
    let fut1 = async {
        loop {
            let value = die.roll();
            info!("VALUE, {:?}", value);
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}
