use defmt::info;
<<<<<<< Updated upstream
use embassy_futures::join::join3;
=======
use embassy_futures::join::{join3, join4};
>>>>>>> Stashed changes
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::{App, Global};
<<<<<<< Updated upstream
use crate::constants::{WAVEFORM_SINE, WAVEFORM_TRIANGLE, WAVEFORM_SAW, WAVEFORM_RECT, CURVE_LOG};
=======
use crate::constants::{CURVE_LOG, CURVE_EXP};
>>>>>>> Stashed changes

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 2;

pub async fn run(app: App<CHANNELS>) {
<<<<<<< Updated upstream
    info!("App AD envelope started on channel: {}", app.channels[0]);


let glob_wave: Global<u16>= app.make_global(0);
let glob_lfo_speed = app.make_global(0.0682);


    let jacks = app.make_all_out_jacks().await;
    

    let mut vals: f32 = 0.0;
=======
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
>>>>>>> Stashed changes

    let fut1 = async {
        loop {

            app.delay_millis(1).await;
<<<<<<< Updated upstream
                
                let lfo_speed = glob_lfo_speed.get().await;
                vals = vals + lfo_speed;
                if vals > 4095.0 {
                    vals = 0.0;
                }
                let wave = glob_wave.get().await;
                
                if wave == 0 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_SINE[vals as usize];
                    jacks.set_values([lfo_pos]);  
                }
                if wave == 1 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_TRIANGLE[vals as usize];
                    jacks.set_values([lfo_pos]);  
                }
                if wave == 2 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_SAW[vals as usize];
                    jacks.set_values([lfo_pos]);  
                }
                if wave == 3 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_RECT[vals as usize];
                    jacks.set_values([lfo_pos]);    
                }           
=======
            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 { //rising edge
                env_state = 1;
                oldinputval = inputval;
            }
            else {
                oldinputval = inputval;
            }

                
                let raise_speed = glob_raise_speed.get().await;
                if env_state == 1{
                    vals = vals + glob_raise_speed.get().await;
                    if vals > 4095.0 {
                        env_state = 2;
                    }
                }
                if env_state == 2{
                    vals = vals - glob_fall_speed.get().await;
                    if vals < 0.0 {
                        env_state = 0;
                        vals = 0.0;
                    }
                }
                output.set_value(vals as u16);
>>>>>>> Stashed changes
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let mut fader = app.get_fader_values();
<<<<<<< Updated upstream
            fader = [CURVE_LOG[fader[0] as usize] as u16];
            info!("Moved fader {} to {}", app.channels[0], fader);
            glob_lfo_speed.set(fader[0] as f32 * 0.004 + 0.0682).await;
=======
            let fader0 = [CURVE_LOG[fader[0] as usize] as u16];
            info!("Moved fader 0 {} to {}", app.channels[0], fader);
            glob_raise_speed.set(fader[0] as f32 * 0.004 + 0.0682).await;
>>>>>>> Stashed changes
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
<<<<<<< Updated upstream
            waiter.wait_for_button_down(0).await;
            let mut wave = glob_wave.get().await;
            wave = wave + 1;
            if wave > 3 {
                wave = 0;
            }
            glob_wave.set(wave).await;
            info!("Wave state {}", wave);
        }
    };

    join3(fut1, fut2, fut3).await;
=======
            waiter.wait_for_fader_change(1).await;
            let mut fader = app.get_fader_values();
            let fader1 = [CURVE_LOG[fader[1] as usize] as u16];
            info!("Moved fader 1 {} to {}", app.channels[1], fader);
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
>>>>>>> Stashed changes
}
