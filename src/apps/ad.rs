
use embassy_futures::join::{join3, join4, join5};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::{App, Global, Range, Led};
use crate::config::{Config, Curve, Param};
use crate::constants::{CURVE_LOG, CURVE_EXP};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 2;

pub async fn run(app: App<CHANNELS>) {

let glob_raise_speed = app.make_global(0.0682);
let glob_fall_speed = app.make_global(0.0682);
let glob_curve0 = app.make_global(0);
let glob_curve1 = app.make_global(0);


    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let minispeed = 0.5;
    let fadstep = 0.05;

    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;
/* 
    TODO:
    LED colours
    LED buttons
    Work on timing and fader response

    */

    let fut1 = async {
        loop {
            let color = (255, 255, 255);
            

            app.delay_millis(1).await;
            
            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 { //detect passing the threshold
                env_state = 1;
                oldinputval = inputval;
            }
            else {
                oldinputval = inputval;
            }

                let raise_speed = glob_raise_speed.get().await;
                if env_state == 1{
                    vals = vals + glob_raise_speed.get().await;
                    if vals > 4094.0 {
                        env_state = 2;
                        vals = 4094.0;
                    }
                    let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];
                    output.set_value_with_curve(curve[glob_curve0.get().await as usize],vals as u16);
                    app.set_led(0, Led::Top, color, (vals as f32 / 16.0) as u8);
                    app.set_led(0, Led::Bottom, color, (255.0 - (vals as f32) / 16.0) as u8);
                    if vals == 4094.0 {
                        app.set_led(0, Led::Top, (0, 0, 0), 0);
                    }
                }

                if env_state == 2{
                    vals = vals - glob_fall_speed.get().await;
                    if vals < 0.0 {
                        env_state = 0;
                        vals = 0.0;

                    }
                    let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];
                    output.set_value_with_curve(curve[glob_curve1.get().await as usize],vals as u16);
                    app.set_led(1, Led::Top, color, (vals as f32 / 16.0) as u8);
                    app.set_led(1, Led::Bottom, color, (255.0 - (vals as f32) / 16.0) as u8);
                    
                    if vals == 0.0 {
                        app.set_led(1, Led::Bottom, (0, 0, 0), 0);
                    }
                  
                }

        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let mut fader = app.get_fader_values();
            fader[0] = 4096 - CURVE_EXP[fader[0] as usize] as u16;

            glob_raise_speed.set(fader[0] as f32 * fadstep + minispeed).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(1).await;
            let mut fader = app.get_fader_values();
            fader[1] = 4096 - CURVE_EXP[fader[1] as usize] as u16;

            glob_fall_speed.set(fader[1] as f32 * fadstep + minispeed).await;
        }
    };

    let fut4 = async {
        let mut waiter = app.make_waiter();
        loop {

            waiter.wait_for_button_down(0).await;
            let mut curve = glob_curve0.get().await;
            curve = curve + 1;
            if curve > 2 {
                curve = 0;
            }
            glob_curve0.set(curve).await;
        }
    };

    let fut5 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(1).await;
            let mut curve = glob_curve1.get().await;
            curve = curve + 1;
            if curve > 2 {
                curve = 0;
            }
            glob_curve1.set(curve).await;
            
        }
    };




    join5(fut1, fut2, fut3, fut4, fut5).await;
}
