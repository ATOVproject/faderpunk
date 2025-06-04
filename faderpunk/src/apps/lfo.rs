use embassy_futures::{
    join::{join, join3},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, Led, Range};
use config::{Config, Waveform};
use libfp::constants::CURVE_LOG;

use super::temp_param_loop;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("LFO", "Wooooosh");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // FIXME: It COULD be that the signal.wait() immediately resolves for some reason
    select(join(run(&app), temp_param_loop()), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let glob_wave = app.make_global(Waveform::Sine);
    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

            let wave = glob_wave.get().await;
            let lfo_speed = glob_lfo_speed.get().await;
            let lfo_pos = glob_lfo_pos.get().await;
            let next_pos = (lfo_pos + lfo_speed) % 4096.0;

            let val = wave.at(next_pos as usize);

            output.set_value(val);

            let color = match wave {
                Waveform::Sine => (243, 191, 78),
                Waveform::Triangle => (188, 77, 216),
                Waveform::Saw => (78, 243, 243),
                Waveform::Rect => (250, 250, 250),
            };

            leds.set(0, Led::Button, color, 75); //75 is good for shooting
            leds.set(0, Led::Top, color, ((val as f32 / 16.0) / 2.0) as u8);
            leds.set(
                0,
                Led::Bottom,
                color,
                ((255.0 - (val as f32) / 16.0) / 2.0) as u8,
            );
            glob_lfo_pos.set(next_pos).await;
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let [fader] = faders.get_values();
            glob_lfo_speed
                .set(CURVE_LOG[fader as usize] as f32 * 0.015 + 0.0682)
                .await;
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let wave = glob_wave.get().await;
            glob_wave.set(wave.cycle()).await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
