use defmt::info;
use embassy_futures::join::{join3, join4};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::{App, Global};
use crate::constants::{WAVEFORM_SINE, WAVEFORM_TRIANGLE, WAVEFORM_SAW, WAVEFORM_RECT, CURVE_LOG};

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    info!("App simple LFO started on channel: {}", app.channels[0]);


let glob_wave: Global<u16>= app.make_global(0);
let glob_lfo_speed = app.make_global(0.0682);
let glob_lfo_pos = app.make_global(0);


    let output = app.make_out_jack(0).await;
    

    let mut vals: f32 = 0.0;

    let fut1 = async {
        loop {

            app.delay_millis(1).await;
                
                let lfo_speed = glob_lfo_speed.get().await;
                vals = vals + lfo_speed;
                if vals > 4095.0 {
                    vals = 0.0;
                }
                let wave = glob_wave.get().await;
                
                if wave == 0 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_SINE[vals as usize];
                    output.set_value(lfo_pos);  
                    //glob_lfo_pos.set(lfo_pos).await;
                    
                }
                if wave == 1 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_TRIANGLE[vals as usize];
                    output.set_value(lfo_pos);  
                    //glob_lfo_pos.set(lfo_pos).await;
                }
                if wave == 2 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_SAW[vals as usize];
                    output.set_value(lfo_pos);  
                    //glob_lfo_pos.set(lfo_pos).await;
                }
                if wave == 3 {
                    let mut lfo_pos;
                    lfo_pos = WAVEFORM_RECT[vals as usize];
                    output.set_value(lfo_pos); 
                    //glob_lfo_pos.set(lfo_pos).await;   
                }           
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let mut fader = app.get_fader_values();
            fader = [CURVE_LOG[fader[0] as usize] as u16];
            //info!("Moved fader {} to {}", app.channels[0], fader);
            glob_lfo_speed.set(fader[0] as f32 * 0.004 + 0.0682).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
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


    let fut4 = async {
    
        loop {
            // app.delay_millis(100).await;
            // app.set_led(0, (glob_lfo_pos.get().await as u8, 0, 0), 50).await;

        }
    };



    join3(fut1, fut2, fut3).await;
}
