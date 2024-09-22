use defmt::info;
use embassy_futures::join::join;
use wmidi::{Channel as MidiChannel, ControlFunction};

use crate::{app::App, tasks::max::MAX_PUBSUB_FADER_CHANGED};

// API ideas:
// - app.wait_for_button_press
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_all_out_jacks().await;
    let fut1 = async {
        loop {
            let [fader] = app.get_fader_values().await;
            jacks.set_values([fader]).await;
            app.delay_millis(100).await;
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter(0);
        loop {
            waiter.wait_for_fader_change().await;
            app.led_blink(0, 100).await;

            let [fader] = app.get_fader_values().await;
            app.midi_send_cc(
                // FIXME: MidiChannel should be configurable (duh)
                MidiChannel::Ch1,
                ControlFunction::GENERAL_PURPOSE_CONTROLLER_1,
                fader,
            )
            .await;
        }
    };

    join(fut1, fut2).await;
}
