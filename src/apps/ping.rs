use defmt::info;

use crate::app::App;
pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let fut1 = async {
        loop {
            info!("PING");
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}