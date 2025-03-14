use defmt::info;
use embassy_futures::join::{join3, join4};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::{App, Global};
use crate::constants::{CURVE_LOG, CURVE_EXP};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 2;

pub async fn run(app: App<CHANNELS>) {
    info!("App simple AD envelope started on channel: {}", app.channels[0]);


let glob_curve: Global<u16>= app.make_global(0);
let glob_raise_speed = app.make_global(0.0682);
let glob_fall_speed = app.make_global(0.0682);


    let input = app.make_in_jack(0).await;
    let output = app.make_out_jack(1).await;
    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;
/* 
    Strat:
    3 state: idle, raising, falling
    input: rising edge 1V thresh -> set to raising

    to do track the rising edge of the input = compare to previous value and see if it goes from under to above the 1V

    */

    let fut1 = async {
        loop {

            app.delay_millis(1).await;
            let inputval = input.get_value();
            

            if inputval >= 406 && oldinputval < 406 { //detect passing the threshold
                env_state = 1;
                oldinputval = inputval;
                info!("env state = {}", env_state);

            }
            else {
                oldinputval = inputval;
            }

                
                let raise_speed = glob_raise_speed.get().await;
                if env_state == 1{
                    vals = vals + glob_raise_speed.get().await;
                    if vals > 4095.0 {
                        env_state = 2;
                        info!("env state = {}", env_state);
                    }
                }
                if env_state == 2{
                    vals = vals - glob_fall_speed.get().await;
                    if vals < 0.0 {
                        env_state = 0;
                        info!("env state = {}", env_state);
                        vals = 0.0;
                    }
                }
                info!("Vals = {}", vals as u16);
                output.set_value(vals as u16);
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let mut fader = app.get_fader_values();
            fader[0] = 4096 - CURVE_EXP[fader[0] as usize] as u16;
            info!("Moved fader {} to {}", app.channels[0], fader[0]);
            glob_raise_speed.set(fader[0] as f32 * 0.004 + 0.0682).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(1).await;
            let mut fader = app.get_fader_values();
            fader[1] = 4096 - CURVE_EXP[fader[1] as usize] as u16;
            info!("Moved fader {} to {}", app.channels[1], fader[1]);
            glob_fall_speed.set(fader[1] as f32 * 0.004 + 0.0682).await;
        }
    };

    let fut4 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            let mut curve = glob_curve.get().await;
            curve = curve + 1;
            if curve > 3 {
                curve = 0;
            }
            glob_curve.set(curve).await;
            info!("curve state {}", curve);
        }
    };



    join4(fut1, fut2, fut3, fut4).await;
}
