use crate::app::{App, Led};

pub const CHANNELS: usize = 1;

pub async fn run(mut app: App<CHANNELS>) {
    let jack = app.make_gate_jack(0, 2048).await;
    let color = (243, 191, 78);
    loop {
        // TODO: We need to implement a waiter for this somehow
        // An app can have as many clock waiters as it has channels
        app.wait_for_clock(24).await;
        jack.set_high().await;
        app.set_led(0, Led::Button, color, 100);
        app.delay_millis(5).await;
        jack.set_low().await;
        app.set_led(0, Led::Button, color, 0);
    }
}
