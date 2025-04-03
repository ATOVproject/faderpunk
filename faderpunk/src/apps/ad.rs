
use defmt::info;
use embassy_futures::join::{join3, join4, join5};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::{App, Global, Range, Led};
use config::{Config, Curve, Param};
use libfp::constants::{CURVE_LOG, CURVE_EXP};
//use crate::constants::{CURVE_LOG, CURVE_EXP};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("AD", "Goes up and then down");


pub async fn run(app: App<CHANNELS>) {


let buttons = app.use_buttons();
let faders = app.use_faders();
let leds = app.use_leds();

let times_glob = app.make_global([0.0682, 0.0682]);
let glob_curve = app.make_global([0, 0]);



    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let minispeed = 10.0;
    let fadstep = 1;

    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;

    let color = [(243, 191, 78), (188, 77, 216), (78, 243, 243)];

    let curve = glob_curve.get().await;
    leds.set(0, Led::Button , color[curve[0]as usize] , 75);
    leds.set(1, Led::Button , color[curve[1]as usize] , 75);

    


    let fut1 = async {
        loop {
            let color = (255, 255, 255);
            

            app.delay_millis(1).await;
            let times = times_glob.get().await;
            let curve_setting = glob_curve.get().await;
            
            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 { //detect passing the threshold
                env_state = 1;
                oldinputval = inputval;
            }
            else {
                oldinputval = inputval;
            }

             
                if env_state == 1{
                    if times[0] == minispeed{
                        vals = 4095.0;
                    }
                   
                    vals =  vals + (4095.0 / times[0]);
                    //info!("value = {}, speed = {}", vals, times[0]);
                    if vals > 4094.0 {
                        env_state = 2;
                        vals = 4094.0;
                    }
                    let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];
                    
                    output.set_value_with_curve(curve[curve_setting[0]],vals as u16);
                    leds.set(0, Led::Bottom, color, (255.0 - (vals as f32) / 32.0) as u8);
                    leds.set(0, Led::Top, color, (vals as f32 / 32.0) as u8);
                    if vals == 4094.0 {
                        leds.set(0, Led::Top, (0, 0, 0), 0);
                        leds.set(0, Led::Bottom, (0, 0, 0), 0);
                    }
                }

                if env_state == 2{
                    vals =  vals - (4095.0 / times[1]);
                    if vals < 0.0 {
                        env_state = 0;
                        vals = 0.0;

                    }
                    let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];
                    output.set_value_with_curve(curve[curve_setting[1]],vals as u16);
                    leds.set(1, Led::Top, color, (vals as f32 / 32.0) as u8);
                    leds.set(1, Led::Bottom, color, (255.0 - (vals as f32) / 32.0) as u8);
                    
                    if vals == 0.0 {
                        leds.set(1, Led::Bottom, (0, 0, 0), 0);
                    }
                  
                }

        }
    };

    let fut2 = async {
        loop {
            
            let mut times = times_glob.get().await;
            let chan = faders.wait_for_any_change().await;
            if chan < 2{
                let vals = faders.get_values();
                times[chan] = CURVE_LOG[vals[chan] as usize] as f32 + minispeed;
                // (4096.0 - CURVE_EXP[vals[chan] as usize] as f32) * fadstep + minispeed;
                times_glob.set(times).await;
                
            }
        }
    };



    let fut3 = async {
        
        loop {
            let chan = buttons.wait_for_any_down().await;
            
            let mut curve_setting = glob_curve.get().await;
            curve_setting[chan] = (curve_setting[chan] + 1) % 3;
            glob_curve.set(curve_setting).await;
            leds.set(chan, Led::Button , color[curve_setting[chan]as usize] , 75);
        }
    };


    join3(fut1, fut2, fut3).await;
}
