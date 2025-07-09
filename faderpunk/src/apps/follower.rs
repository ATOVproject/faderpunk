use config::Config;
use embassy_futures::{join::join3, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::app::{App, AppStorage, Led, ManagedStorage, Range, RGB8};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons");

const LED_COLOR: RGB8 = RGB8 {
    r: 188,
    g: 77,
    b: 216,
};
const BUTTON_BRIGHTNESS: u8 = 75;

#[derive(Serialize, Deserialize, Default)]
pub struct Storage {
    muted: bool,
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            run(&app, storage).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let muted = storage.query(|s| s.muted).await;
    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );
    leds.set(
        1,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );
    let _input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let _output = app.make_out_jack(1, Range::_0_10V).await;

    let slew_glob = app.make_global(1);
    let slew_mult_glob = app.make_global(0);
    let att_glob = app.make_global(0);

    //slew_glob.load().await;
    //att_glob.load().await;

    let mut buffer = [0; 2048];
    let mut outval = 0;

    let fut1 = async {
        loop {
            let slew = slew_glob.get().await;

            app.delay_millis(1).await;
            let mut inval = _input.get_value();
            inval = rectify(inval); //rectify the values
            buffer = shift_and_insert(buffer, inval, outval, slew);
            outval = average_values(&buffer, slew as usize);
            leds.set(0, Led::Top, LED_COLOR, ((outval / 16) / 2) as u8);
            leds.set(0, Led::Bottom, LED_COLOR, ((255 - (outval) / 16) / 2) as u8);

            let att = att_glob.get().await;
            outval = dynamic_scale(outval, att);
            leds.set(1, Led::Top, LED_COLOR, ((outval / 16) / 2) as u8);
            leds.set(1, Led::Bottom, LED_COLOR, ((255 - (outval) / 16) / 2) as u8);
            _output.set_value(outval);
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let mut vals = faders.get_values();
            if chan == 0 {
                let slew_mult = slew_mult_glob.get().await;
                vals[chan] = vals[chan] / (2 + (slew_mult * 2)) + 1;
                slew_glob.set(vals[chan]).await;
            }

            if chan == 1 {
                att_glob.set(vals[chan]).await
            }
        }
    };

    let fut3 = async {
        loop {
            let chan = buttons.wait_for_any_down().await;
            if chan == 0 {
                // let mut slew_mult = slew_mult_glob.get().await;
                // slew_mult += 1;
                // slew_mult = slew_mult % 2;
                // slew_mult_glob.set(slew_mult).await;
            }
        }
    };

    join3(fut1, fut2, fut3).await;
}

fn shift_and_insert(input: [u16; 2048], new_value: u16, avr: u16, slew: u16) -> [u16; 2048] {
    let mut output = [0; 2048];
    output[0] = new_value;
    for i in 1..2048 {
        output[i] = input[i - 1];
        if i > slew as usize {
            output[i] = avr;
        }
    }
    output
}

fn average_values(data: &[u16], count: usize) -> u16 {
    let count = count.clamp(0, data.len());

    let sum: u32 = data.iter().take(count).map(|&v| v as u32).sum();
    (sum / count as u32) as u16
}

fn dynamic_scale(input: u16, modulation: u16) -> u16 {
    let input = input as i32;
    let mod_val = modulation as i32;

    // Map modulation (0..=4095) to a blend factor from -1.0 (invert) to +1.0 (normal)
    let blend = (mod_val - 2047) as f32 / 2048.0;

    // Normal = input, Inverted = 4095 - input
    let normal = input as f32;
    let inverted = (4095 - input) as f32;

    // Interpolate between inverted and normal
    let result = inverted * (1.0 - blend) / 2.0 + normal * (1.0 + blend) / 2.0;

    result.clamp(0.0, 4095.0) as u16
}

fn rectify(value: u16) -> u16 {
    if value <= 2047 {
        ((2047 - value) as u32 * 4095 / 2047) as u16
    } else if value <= 4095 {
        ((value - 2048) as u32 * 4095 / 2047) as u16
    } else {
        0 // fallback, should never happen
    }
}
