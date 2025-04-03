use config::{Config, Curve, Param};
use embassy_futures::join::join3;
use midi2::ux::u4;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

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

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;
    let curve = config.get_curve_at(0);
    let midi_channel = u4::new(config.get_int_at(1) as u8);

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(midi_channel);
    let mut clock = app.use_clock();

    let rec_flag = app.make_global(false);
    let jack = app.make_out_jack(0, Range::_0_10V).await;
    
    let mut index= 0;
    let mut recording = false;
    let mut buffer = [0; 16];

    let fut1 = async {
        loop {
            clock.wait_for_tick(1).await;
            index += 1;
            index = index % 16;
            
            if index == 0 {
                recording = false;
            }

            if rec_flag.get().await  {
                index = 0;
                recording = true;
            }

            

            if recording && buttons.is_button_pressed(0){
                let val = faders.get_values();
                buffer[index] = val[0];
            }
            
            if !recording{
                jack.set_value(buffer[index]);
            }

        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;            
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            rec_flag.set(true).await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
