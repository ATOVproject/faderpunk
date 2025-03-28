use embassy_futures::join::join3;

use crate::app::{App, Led, Range};
use config::{Config, Waveform};
use libfp::constants::CURVE_LOG;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("LFO", "Wooooosh");

pub async fn run(app: App<CHANNELS>) {
    let glob_wave = app.make_global(Waveform::Sine);
    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);

    let output = app.make_out_jack(0, Range::_0_10V).await;
    let faders = app.use_faders();
    let buttons = app.use_buttons();

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
                Waveform::Sine => (156, 84, 179),
                Waveform::Triangle => (223, 179, 75),
                Waveform::Saw => (68, 247, 246),
                Waveform::Rect => (15, 108, 189),
            };

            app.set_led(0, Led::Button, color, (val as f32 / 16.0) as u8);
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
            // TODO: debounce
            // button.debounce().await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
