use defmt::info;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use libfp::{Brightness, Config, Curve, Param};

use crate::app::{App, Led, RGB8};

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("RGB test app", "Fader set color");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let app_loop = async {
        loop {
            run(&app).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let fader = app.use_faders();
    let leds = app.use_leds();

    let mut color = [255; 3];
    let intensities = [
        Brightness::Lowest,
        Brightness::Lower,
        Brightness::Low,
        Brightness::Default,
    ];
    loop {
        let chan = fader.wait_for_any_change().await;
        let val = fader.get_all_values();
        color[chan] = (val[chan] / 16) as u8;
        let rgb: smart_leds::RGB<u8> = RGB8 {
            r: color[0],
            g: color[1],
            b: color[2],
        };

        for (i, &intensity) in intensities.iter().enumerate() {
            leds.set(i, Led::Top, rgb, intensity);
            leds.set(i, Led::Bottom, rgb, intensity);
            leds.set(i, Led::Button, rgb, intensity);
        }
        info!("R: {}, G: {}, B: {}", color[0], color[1], color[2])
    }
}
