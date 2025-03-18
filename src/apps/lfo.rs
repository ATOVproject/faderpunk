use defmt::info;
use embassy_futures::join::{join3, join4};

use crate::app::{App, Global, Range};
use crate::constants::{CURVE_LOG, WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SINE, WAVEFORM_TRIANGLE};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    info!("App simple LFO started on channel: {}", app.channels[0]);

    let glob_wave: Global<u16> = app.make_global(0);
    let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0);

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;

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

            app.set_led(0, Led::Button, color, 200);
            app.set_led(0, Led::Top, color, (val as f32 / 16.0) as u8);
            app.set_led(0, Led::Bottom, color, (255.0 - (val as f32) / 16.0) as u8);
            glob_lfo_pos.set(next_pos).await;
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let mut fader = app.get_fader_values();
            fader = [CURVE_LOG[fader[0] as usize] as u16];
            //info!("Moved fader {} to {}", app.channels[0], fader);
            glob_lfo_speed.set(fader[0] as f32 * 0.015 + 0.0682).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            let mut wave = glob_wave.get().await;
            wave = wave + 1;
            if wave > 3 {
                wave = 0;
            }
            glob_wave.set(wave).await;
            info!("Wave state {}", wave);
        }
    };

    let fut4 = async {
        loop {
            app.delay_millis(50).await;
            app.set_led(0, (50, 0, 0), 50).await;
        }
    };

    join4(fut1, fut2, fut3, fut4).await;
}
