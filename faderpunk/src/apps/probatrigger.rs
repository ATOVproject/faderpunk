use config::Config;
use embassy_futures::{join::join3, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, ClockEvent, Led, RGB8};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("LFO", "Wooooosh");

const LED_COLOR: RGB8 = RGB8 {
    r: 78,
    g: 243,
    b: 243,
};

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    //let midi = app.use_midi(10);
    let mut clock = app.use_clock();
    let mut die = app.use_die();

    let jack = app.make_gate_jack(0, 2048).await;

    let glob_muted = app.make_global(false);
    let fad_init = fader.get_value();
    let fad_glob = app.make_global(fad_init);
    leds.set(0, Led::Button, LED_COLOR, 75);

    let fut1 = async {
        loop {
            if let ClockEvent::Tick = clock.wait_for_event(1).await {
                let val = fad_glob.get().await;
                let rndval = die.roll();
                if val >= rndval && !glob_muted.get().await {
                    jack.set_high().await;
                    leds.set(0, Led::Top, LED_COLOR, 75);
                    //midi.send_note_on(75 ,4095);
                    app.delay_millis(20).await;
                    jack.set_low().await;
                    leds.set(0, Led::Top, LED_COLOR, 0);
                    //midi.send_note_off(75 as u8);
                }
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                leds.set(0, Led::Button, LED_COLOR, 40);
                leds.set(0, Led::Top, LED_COLOR, 0);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 75);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change().await;
            let val = fader.get_value();
            fad_glob.set(val).await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
