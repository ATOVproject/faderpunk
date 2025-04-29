use crate::app::App;

pub const CHANNELS: usize = 1;

app_config! (
    config("Trigger", "Test app to test the clock and GPOs");
    params();
    storage();
);

pub async fn run(app: App<'_, CHANNELS>, _ctx: &AppContext<'_>) {
    let jack = app.make_gate_jack(0, 2048).await;
    let mut clock = app.use_clock();
    // let color = (243, 191, 78);
    loop {
        // TODO: We need to implement a waiter for this somehow
        // An app can have as many clock waiters as it has channels
        clock.wait_for_tick(1).await;
        jack.set_high().await;
        // TODO: We need an app.led_blink or something, otherwise one won't be able to see the led
        // blink
        // app.set_led(0, Led::Button, color, 100);
        // TODO: We should also have a trigger function that adjusts the trigger length to the
        // clock
        app.delay_millis(5).await;
        jack.set_low().await;
        // app.set_led(0, Led::Button, color, 0);
    }
}
