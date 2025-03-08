use defmt::info;
use embassy_futures::join::join3;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::App;

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;
/*
Strategy:
Measure the Fader value every so often
Calculate by how many u16 value has to change per 1ms (LFO speed) refresh rate of the DAC 
    Function of fader value * by some constant to get the desired range (0.5s - 1min ??)
add with warp to the lfo_pos
reduce lfo_pos to 12bits
lookup the DAC value for the waveform
Button cycle through waveform tables


Needed: 
Do the math
Wavetables: 
-Saw (not needed can be lfo_pos)
-Tri
-Sine

 */
pub async fn run(app: App<CHANNELS>) {
    

    
    let lfo_pos: u16;
    let mut vals: [u16; 1]= [0];
    let jacks = app.make_all_out_jacks().await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
        }
    };

    let fut2 = async {

        loop {
            app.delay_millis(1).await;
            vals[0] += vals[0];
            if vals[0] > 4095 {
                vals[0] = 0;
            }
            jacks.set_values(vals);
  
        }
    };

    let fut3 = async {
    
        loop {
            
        }
    };

    join3(fut1, fut2, fut3).await;
}
