use defmt::info;
use embassy_futures::join::join3;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

use crate::app::App;

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    info!("App default started on channel: {}", app.channels[0]);

    let glob_muted: Mutex<NoopRawMutex, bool> = Mutex::new(false);

    let jacks = app.make_all_out_jacks().await;
    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            let muted = glob_muted.lock().await;
            if !*muted {
                let vals = app.get_fader_values();
                jacks.set_values(vals);
            }
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_fader_change(0).await;
            let [fader] = app.get_fader_values();
            info!("Moved fader {} to {}", app.channels[0], fader);
            // app.midi_send_cc(0, fader).await;
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            info!("Pressed button {}", app.channels[0]);
            let mut muted = glob_muted.lock().await;
            *muted = !*muted;
            if *muted {
                jacks.set_values([0]);
            }
        }
    };

    join3(fut1, fut2, fut3).await;
}
