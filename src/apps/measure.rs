use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_all_in_jacks().await;
    let fut1 = async {
        loop {
            let values = jacks.get_values().await;
            info!("VALUES, {:?}", values);
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}
