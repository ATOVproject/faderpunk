use crate::app::{App, Led};

pub const CHANNELS: usize = 16;

app_config! (
    config("Clock test", "Visualize clock tempo");
    params();
    storage();
);

pub async fn run(app: App<CHANNELS>, _ctx: &AppContext<'_>) {
    let mut clock = app.use_clock();
    let color = (243, 191, 78);
    let leds = app.use_leds();
    let mut cur: usize = 0;
    loop {
        clock.wait_for_tick(1).await;
        leds.set(cur, Led::Button, color, 100);
        loop {
            let is_reset = clock.wait_for_tick(6).await;
            if is_reset {
                leds.set(cur, Led::Button, color, 0);
                cur = 0;
                break;
            } else {
                cur = (cur + 1) % 16;
                leds.set(cur, Led::Button, color, 100);
            }
            let prev = if cur == 0 { 15 } else { cur - 1 };
            leds.set(prev, Led::Button, color, 0);
        }
    }
}
