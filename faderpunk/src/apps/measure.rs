use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;

app_config! (
    config("Measure", "Test app to measure port voltages");
    params();
);

pub async fn run(app: App<CHANNELS>, _params: AppParams<'_>) {
    let mut die = app.use_die();
    let fut1 = async {
        loop {
            let value = die.roll();
            info!("VALUE, {:?}", value);
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}
