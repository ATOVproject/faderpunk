use defmt::info;
use embassy_futures::join::{join, join3};
use wmidi::{Channel as MidiChannel, ControlFunction};

use crate::app::App;

// API ideas:
// - app.wait_for_button_press
// - app.wait_for_midi_on_channel

// FIXME: How to implement waiters?

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_all_out_jacks().await;
    let fut1 = async {
        loop {
            let [fader] = app.get_fader_values().await;
            jacks.set_values([fader]).await;
            app.delay_millis(10).await;
        }
    };

    // let fut2 = async {
    //     let mut waiter = app.make_fader_waiter(0);
    //     loop {
    //         waiter.wait_for_fader_change().await;
    //         app.led_blink(0, 100).await;
    //
    //         // FIXME: Sending MIDI messages into the void is still not possible
    //         // let [fader] = app.get_fader_values().await;
    //         // app.midi_send_cc(
    //         //     // FIXME: MidiChannel should be configurable (duh)
    //         //     MidiChannel::Ch1,
    //         //     ControlFunction::GENERAL_PURPOSE_CONTROLLER_1,
    //         //     fader,
    //         // )
    //         // .await;
    //     }
    // };

    // let fut3 = async {
    //     let mut waiter = app.make_button_waiter(0);
    //     loop {
    //         waiter.wait_for_button_press().await;
    //         app.led_blink(0, 100).await;
    //     }
    // };

    fut1.await;

    // join3(fut1, fut2, fut3).await;
}
