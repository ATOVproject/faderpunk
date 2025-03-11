use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_in_jack(0).await;
    let fut1 = async {
        loop {
            let values = jacks.get_value();
            info!("VALUES, {:?} on {}", values, app.channels[0]);
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}
