use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use fp_core::app::{App, Led};
use libfp::{AppIcon, Brightness, Color, Config};

pub const CHANNELS: usize = 1;
pub static CONFIG: Config<0> = Config::new(
    "External Example",
    "Brightness follows the fader",
    Color::Cyan,
    AppIcon::Fader,
);

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), app.exit_handler(exit_signal)).await;
}

async fn run(app: &App<CHANNELS>) {
    let faders = app.use_faders();
    let leds = app.use_leds();
    loop {
        let brightness = Brightness::Custom((faders.get_value_at(0) >> 4) as u8);
        leds.set(0, Led::Top, Color::Cyan, brightness);
        faders.wait_for_change_at(0).await;
    }
}
