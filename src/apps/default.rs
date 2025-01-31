use defmt::info;
use embassy_futures::join::{join, join3};
use wmidi::{Channel as MidiChannel, ControlFunction, U7};

use crate::app::App;

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_all_out_jacks().await;
    let fut1 = async {
        info!("APP CHANNEL: {}", app.channels[0]);
        loop {
            app.delay_millis(10).await;
            let vals = app.get_fader_values();
            jacks.set_values(vals);
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

    // let fut3 = async {
    //     let mut waiter = app.make_button_waiter(0);
    //     loop {
    //         waiter.wait_for_button_press().await;
    //         app.led_blink(0, 100).await;
    //     }
    // };

    let fut3 = async {
        loop {
            info!("APP HEARTBEAT, CHAN {}", app.channels[0]);
            app.delay_secs(4).await;
        }
    };

    // join(fut1, fut3).await;
    //
    join3(fut1, fut2, fut3).await;
}
