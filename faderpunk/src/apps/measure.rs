use defmt::info;
use embassy_futures::{join::join, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::App;
use config::Config;

use super::temp_param_loop;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> =
    Config::new("Measure", "Test app to measure port voltages");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(join(run(&app), temp_param_loop()), exit_signal.wait()).await;
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
