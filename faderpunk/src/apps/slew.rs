use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::join3;

use crate::app::{App, Led, Range, StorageSlot};

use libfp::constants::{CURVE_LOG, CURVE_EXP};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 3;

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        default: Curve::Linear,
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    .add_param(Param::Int {
        name: "Midi channel",
        default: 0,
        min: 0,
        max: 15,
    });

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

    let mut glob_muted = app.make_global_with_store(false, StorageSlot::A);
    glob_muted.load().await;

    let muted = glob_muted.get().await;
    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );
    let _input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let _output = app.make_out_jack(1, Range::_Neg5_5V).await;

    let slew_glob = app.make_global(1);
    let mut att_glob = app.make_global(0);

    //slew_glob.load().await;
    //att_glob.load().await;

    let mut buffer = [0; 2048];


    let fut1 = async {
        loop {
            let slew = slew_glob.get().await;

            app.delay_millis(1).await;
            let inval = _input.get_value();
            buffer = shift_and_insert(buffer, inval);
            let mut outval = average_values(&buffer, slew as usize);
            let att = att_glob.get().await;
            outval = dynamic_scale(outval, att);         
            _output.set_value(outval);
            //info!("in = {}, out = {}", inval, outval)
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let mut vals = faders.get_values();
            if chan == 0 {
                vals[chan] =  CURVE_LOG[vals[chan] as usize];
                vals[chan] = (vals[chan] / 2 + 1);
                slew_glob.set(vals[chan]).await;
            }

            if chan == 1 {
                att_glob.set(vals[chan]).await
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_any_down().await;
            
        }
    };

    join3(fut1, fut2, fut3).await;
}



fn shift_and_insert(input: [u16; 2048], new_value: u16) -> [u16; 2048] {
    let mut output = [0; 2048];
    output[0] = new_value;
    for i in 1..2048{
        output[i] = input[i - 1];
    }
    output
}

fn average_values(data: &[u16], count: usize) -> u16 {
    if count == 0 || count > data.len() {
        panic!("Invalid count: must be between 1 and data length.");
    }

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

