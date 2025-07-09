use config::{Config, Curve, Param, Value};
use defmt::info;
use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

use crate::app::{App, ClockEvent, Led, ParamSlot, ParamStore, Range, RGB8};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<PARAMS> = Config::new(
    "Turing",
    "Classic turing machine, synched to internal clock",
)
.add_param(Param::Curve {
    //I want to be able to choose between none, CC and note
    name: "MIDI",
    variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
})
.add_param(Param::i32 {
    //is it possible to have this apear only if CC or note are selected
    name: "Midi channel",
    min: 0,
    max: 15,
});
// .add_param(Param::i32 {
//     //is it possible to have this apear only if CC
//     name: "CC Number",
//     min: 0,
//     max: 127,
// })
// .add_param(Param::i32 {
//     //is it possible to have this apear only if CC
//     name: "Scale",
//     min: 0,
//     max: 127,
// });

const LED_COLOR: RGB8 = RGB8 {
    r: 0,
    g: 200,
    b: 150,
};
const BUTTON_BRIGHTNESS: u8 = 75;

pub struct Params<'a> {
    curve: ParamSlot<'a, Curve, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [Value::Curve(Curve::Linear), Value::i32(1)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        curve: ParamSlot::new(&param_store, 0),
        midi_channel: ParamSlot::new(&param_store, 1),
    };

    let app_loop = async {
        loop {
            select(run(&app, &params), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let mut die = app.use_die();

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut amp_glob = app.make_global_with_store(4095, StorageSlot::C);

    let prob_glob = app.make_global(0);
    let length_glob = app.make_global(15_u16);
    let amp_glob = app.make_global(4095);

    let mut shift_old = false;

    let latched_glob = app.make_global(true);

    let mut register = die.roll();

    leds.set(0, Led::Button, LED_COLOR, 100);

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let fut1 = async {
        loop {
            if let ClockEvent::Tick = clock.wait_for_event(6).await {
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
            faders.wait_for_change(0).await;
            let vals = faders.get_values();
            let length = length_glob.get().await;
            let amp = amp_glob.get().await;
            let prob = prob_glob.get().await;

            if buttons.is_shift_pressed() {
                let val = return_if_close(length as u16, vals[0] / 273 + 1);
                if val.1 {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    length_glob.set(val.0).await;
                    info! {"{}", val.0}
                }
            }
            if buttons.is_button_pressed(0) {
                let val = return_if_close(amp as u16, vals[0]);
                if val.1 {
                    latched_glob.set(true).await;
                }

                if latched_glob.get().await {
                    amp_glob.set(val.0 as u32).await;
                    info! {"{}", val.0}
                }
            } else {
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
            // do the slides here
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
