use defmt::info;
use embassy_futures::join::join3;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::App;

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    info!("App default started on channel: {}", app.channels[0]);

    let glob_muted: Mutex<NoopRawMutex, bool> = Mutex::new(false);

    let jacks = app.make_all_out_jacks().await;
    

    let mut vals: f32 = 0.0;
    let fut1 = async {
        loop {

            app.delay_millis(1).await;
                let fader = app.get_fader_values();
                let lfo_speed = fader[0] as f32 * 0.004 + 0.0682; //recalculate this only when a new fader value is read
                vals = vals + lfo_speed;
                if vals > 4095.0 {
                    vals = 0.0;
                }
                let lfo_pos: [u16; 1] = [vals as u16];
                jacks.set_values(lfo_pos);            
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let [fader] = app.get_fader_values();
            info!("Moved fader {} to {}", app.channels[0], fader);
            // let cc_chan = U7::from_u8_lossy(102 + app.channels[0] as u8);
            // app.midi_send_cc(MidiChannel::Ch1, ControlFunction(cc_chan), fader)
            //     .await;

        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
    
        }
    };

    join3(fut1, fut2, fut3).await;
}
