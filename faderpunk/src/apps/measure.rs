use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let mut die = app.make_die();
    let fut1 = async {
        loop {
            let value = die.roll();
            info!("VALUE, {:?}", value);
            app.delay_millis(2000).await;
        }
    };

    fut1.await;
}
