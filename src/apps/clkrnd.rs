use crate::app::{App, Led};

pub const CHANNELS: usize = 1;

pub async fn run(mut app: App<CHANNELS>) {
    let jack = app.make_out_jack(0).await;
    // let color = (243, 191, 78);
    loop {
        // TODO: We need to implement a waiter for this somehow
        // An app can have as many clock waiters as it has channels
        app.wait_for_clock(24).await;
        jack.set

        // app.set_led(0, Led::Button, color, 0);
    }
}
