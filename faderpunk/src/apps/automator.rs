use config::{Config, Curve, Param};
use embassy_futures::join::join3;
use embassy_rp::trng::InverterChainLength;
use midi2::ux::u4;

use crate::{app::{App, Led, Range}, tasks::clock};

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
    let mut _clock = app.use_clock();

    let rec_glob = app.make_global(false);
    leds.set(0, Led::Button, LED_COLOR, 75);

  

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let fut1 = async {
        loop {
            _clock.wait_for_tick(1).await;
            if buttons.is_button_pressed(0) {
                rec_glob.set(true).await;
            }  
            else {
                rec_glob.set(false).await;
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            if !rec_glob.get().await {

            }
        }
    };

let mut index = 0;
let mut old_rec = false ;
let mut recording = false;
let mut lenght = 1;
let mut start = 0;
let mut buffer: [u16; 800] = [0; 800];

    let fut3 = async {
        loop {
            if old_rec != rec_glob.get().await && !old_rec{
                recording = !recording;
                index = 0;
                old_rec = true;
            }

            if old_rec != rec_glob.get().await && old_rec{
                recording = !recording;
                lenght = index;                
            }



        
            app.delay_millis(10).await;
            index += 1;
            index  = index % 800;
            if index == 0 {
                recording = false;
            }

            if recording {
                let val = faders.get_values();
                buffer[index] = val[0];
                jack.set_value(val[0]);
            }

            if !recording {
                
            }

            
        }
    };



    join3(fut1, fut2, fut3).await;
}
