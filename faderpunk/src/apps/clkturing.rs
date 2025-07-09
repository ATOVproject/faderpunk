//TODO:
// Set fader function
// Add trig input

use config::Config;
use defmt::info;
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, Led, Range, RGB8};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Turing",
    "Classic turing machine, synched to internal clock",
);

const LED_COLOR: RGB8 = RGB8 {
    r: 0,
    g: 200,
    b: 150,
};

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let mut die = app.use_die();

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut amp_glob = app.make_global_with_store(4095, StorageSlot::C);

    let prob_glob = app.make_global(0);
    let length_glob = app.make_global(15_u16);
    let amp_glob = app.make_global(4095);

    let mut shift_old = false;

    let latched_glob = app.make_global(true);
    let register_glob = app.make_global(0);
    register_glob.set(die.roll()).await;
    let mut oldinputval = 0;
    let mut tick = false;

    leds.set(0, Led::Button, LED_COLOR, 100);

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let jack = app.make_out_jack(1, Range::_0_10V).await;
    let fut1 = async {
        loop {
            //clock.wait_for_tick(6).await;
            app.delay_millis(1).await;
            let mut register = register_glob.get().await;

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                //detect passing the threshold
                tick = true;
                oldinputval = inputval;
            } else {
                oldinputval = inputval;
            }

            if tick {
                //info!("tick!");
                let prob = prob_glob.get().await;
                let mut rand = die.roll();
                rand = (rand as u32 * 4000 / 4095 + 40) as u16;
                let length = length_glob.get().await;
                let rotation = rotate_select_bit(register, prob, rand, length);
                register = rotation.0;
                //info!("{:016b}, flip {}, rnd {}", register, rotation.1, rand);
                let register_scalled = scale_to_12bit(register, length as u8);
                let att_reg = register_scalled as u32 * amp_glob.get().await / 4095;
                jack.set_value(att_reg as u16);
                tick = false;
                register_glob.set(register).await;
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let length = length_glob.get().await;
            let prob = prob_glob.get().await;

            if buttons.is_shift_pressed() && chan == 0 {
                let val = vals[0] / 273 + 1;
                if val == length {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    length_glob.set(val).await;
                    info! {" length {}", val}
                }
            }
            if chan == 1 {
                amp_glob.set(vals[1] as u32).await;
            }
            if chan == 0 && !buttons.is_shift_pressed() {
                let val = return_if_close(prob, vals[0]);

                if val.1 {
                    latched_glob.set(true).await;
                }

                if latched_glob.get().await {
                    prob_glob.set(val.0).await;
                    info! {"prob {}", val.0}
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let mut die2 = app.use_die();
            register_glob.set(die2.roll()).await;
        }
    };

    let fut4 = async {
        loop {
            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = true;
                info!("unlatch everything")
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = false;
                info!("unlatch everything again")
            }
        }
    };

    join4(fut1, fut2, fut3, fut4).await;
}

fn rotate_select_bit(x: u16, a: u16, b: u16, bit_index: u16) -> (u16, bool) {
    let bit_index = bit_index.clamp(0, 15);

    // Extract the original bit
    let original_bit = ((x >> bit_index) & 1) as u8;
    let mut bit = original_bit;

    // Invert the bit if a > b
    if a > b {
        bit ^= 1;
    }

    // Shift x left by 1, and keep it within 16 bits
    let shifted = x << 1;

    // Insert the (possibly inverted) bit into the LSB
    let result = shifted | (bit as u16);

    // Return the new value and whether the bit was flipped
    let flipped = bit != original_bit;
    (result, flipped)
}

fn scale_to_12bit(input: u16, x: u8) -> u16 {
    let x = x.clamp(1, 16);

    // Shift to keep the top `x` bits
    let top_x_bits = input >> (16 - x);

    // Scale to 12-bit
    let max_x_val = (1 << x) - 1;
    ((top_x_bits as u32 * 4095) / max_x_val as u32) as u16
}

fn return_if_close(a: u16, b: u16) -> (u16, bool) {
    if a.abs_diff(b) < 50 {
        (b, true)
    } else {
        // TODO: Does it make sense to return b in both cases?
        (b, false)
    }
}
