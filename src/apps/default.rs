use crate::{App, Timer};

pub async fn run(app: App<1>) {
    loop {
        Timer::after_millis(1000).await;
        let [val] = app.get_fader_values().await;
        app.set_dac_values([val]).await;
    }
}
