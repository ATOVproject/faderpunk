//Todo:
//Fader read applies value always
// optional : lock array to the grid for recall.

use config::Config;
use embassy_futures::{join::join3, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("Automator", "Fader movement recording");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(4);
    let mut clock = app.use_clock();

    let rec_flag = app.make_global(false);
    let del_flag = app.make_global(false);
    let offset_glob = app.make_global(0);
    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let mut last_midi = 0;

    let mut index = 0;
    let mut recording = false;
    let mut buffer = [0; 384];
    let mut length = 384;
    let color = (255, 255, 255);

    leds.set(0, Led::Button, (255, 255, 255), 0);

    let fut1 = async {
        loop {
            let reset = clock.wait_for_tick(1).await;

            index += 1;
            //info!("reset = {}", reset);
            if reset {
                index = 0;
                recording = false;
                //info!("reset");
            }

            //info!("clock");
            index = index % length;

            if index == 0 && recording {
                //stop recording at max c
                recording = false;
            }

            if rec_flag.get().await {
                index = 0;
                recording = true;
                rec_flag.set(false).await;
                length = 384;
                offset_glob.set(0).await;
            }

            if recording {
                let val = faders.get_values();
                buffer[index] = val[0];
                jack.set_value(buffer[index]);
                if last_midi / 16 != (buffer[index]) / 16 {
                    midi.send_cc(0, buffer[index]).await;
                    last_midi = buffer[index];
                }

                midi.send_cc(0, buffer[index]).await;
                leds.set(0, Led::Button, (255, 0, 0), 100);
                leds.set(0, Led::Top, (255, 0, 0), (buffer[index] / 32) as u8);
                leds.set(
                    0,
                    Led::Bottom,
                    (255, 0, 0),
                    (255 - (buffer[index] / 16) as u8) / 2,
                )
            }

            if recording && !buttons.is_button_pressed(0) && index % 96 == 0 && index != 0 {
                recording = !recording;
                length = index;
            }

            if !recording {
                let offset = offset_glob.get().await;
                let mut val: u16 = buffer[index] + offset;
                if val > 4095 {
                    val = 4095;
                }
                jack.set_value(val);
                if last_midi / 16 != (val) / 16 {
                    midi.send_cc(0, val).await;
                    last_midi = val;
                }
                leds.set(0, Led::Button, color, 100);
                leds.set(0, Led::Top, color, ((buffer[index] + offset) / 16) as u8);
                leds.set(
                    0,
                    Led::Bottom,
                    color,
                    (255 - ((buffer[index] + offset) / 16) as u8) / 2,
                );
            }

            if index == 0 {
                leds.set(0, Led::Button, (255, 255, 255), 0);
            }

            if del_flag.get().await {
                for n in 0..383 {
                    buffer[n] = 0;
                }
                recording = false;
                offset_glob.set(0).await;
                del_flag.set(false).await;
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let val = faders.get_values();
            offset_glob.set(val[0]).await;
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            if buttons.is_shift_pressed() {
                del_flag.set(true).await;
            } else {
                rec_flag.set(true).await;
            }
        }
    };

    join3(fut1, fut2, fut3).await;
}
