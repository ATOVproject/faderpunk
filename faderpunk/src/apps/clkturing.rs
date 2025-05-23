use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::{join3, join4};

use crate::app::{App, Led, Range, InJack};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 3;

//TODO:
// Set fader function
// Add trig input

pub static CONFIG: Config<PARAMS> = Config::new(
    "Turing",
    "Classic turing machine, synched to internal clock",
);
//     .add_param(Param::Curve { //I want to be abl to choose between none, CC and note
//         name: "MIDI",
//         default: Curve::Linear,
//         variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
//     })
//     .add_param(Param::Int { //is it possible to have this apear only if CC or note are selected
//         name: "Midi channel",
//         default: 0,
//         min: 0,
//         max: 15,
//     })
//     .add_param(Param::Int { //is it possible to have this apear only if CC?
//         name: "CC Number",
//         default: 0,
//         min: 0,
//         max: 127,
//     })
//     .add_param(Param::Int { //Scale
//         name: "Scale",
//         default: 0,
//         min: 0,
//         max: 127,
//     });

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;
    // TODO: Maybe rename: get_curve_from_param(idx)
    let curve = config.get_curve_at(0);
    let midi_channel = config.get_int_at(1) as u8;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(midi_channel);
    let mut clock = app.use_clock();
    let mut die = app.use_die();

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut amp_glob = app.make_global_with_store(4095, StorageSlot::C);

    let mut prob_glob = app.make_global(0);
    let mut length_glob = app.make_global(15);
    let mut amp_glob = app.make_global(4095);

    let mut shift_old = false;

    let latched_glob = app.make_global(true);

    let mut register = die.roll();
    let mut oldinputval = 0;
    let mut tick= false;

    leds.set(0, Led::Button, LED_COLOR, 100);

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let jack = app.make_out_jack(1, Range::_0_10V).await;
    let fut1 = async {
        loop {
            //clock.wait_for_tick(6).await;
            app.delay_millis(1).await;

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                //detect passing the threshold
                tick = true;
                oldinputval = inputval;
            } else {
                oldinputval = inputval;
            }

            if tick {
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
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let length = length_glob.get().await;
            let amp = amp_glob.get().await;
            let prob = prob_glob.get().await;

            if buttons.is_shift_pressed() && chan == 1{
                let val = return_if_close(length, vals[0] / 273 + 1);
                if val.1 {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    length_glob.set(val.0).await;
                    info! {"{}", val.0}
                }
            }
            if !buttons.is_shift_pressed() && chan == 1 {
                let val = return_if_close(amp as u16, vals[0]);
                if val.1 {
                    latched_glob.set(true).await;
                }

                if latched_glob.get().await {
                    amp_glob.set(val.0 as u32).await;
                    info! {"{}", val.0}
                }
            } 
            if chan == 0 {
                let val = return_if_close(prob, vals[0]);

                if val.1 {
                    latched_glob.set(true).await;
                    info!("latched")
                }

                if latched_glob.get().await {
                    prob_glob.set(val.0).await;
                    info! {"{}", val.0}
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            latched_glob.set(false).await;
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
    if bit_index > 15 {
        panic!("bit_index must be between 0 and 15");
    }

    // Extract the original bit
    let original_bit = ((x >> bit_index) & 1) as u8;
    let mut bit = original_bit;

    // Invert the bit if a > b
    if a > b {
        bit ^= 1;
    }

    // Shift x left by 1, and keep it within 16 bits
    let shifted = (x << 1) & 0xFFFF;

    // Insert the (possibly inverted) bit into the LSB
    let result = shifted | (bit as u16);

    // Return the new value and whether the bit was flipped
    let flipped = bit != original_bit;
    (result, flipped)
}

fn scale_to_12bit(input: u16, x: u8) -> u16 {
    assert!(x > 0 && x <= 16, "x must be between 1 and 16");

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
        (b, false)
    }
}
