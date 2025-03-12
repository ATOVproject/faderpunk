use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 2;

pub async fn run(app: App<CHANNELS>) {
    let inputjack = app.make_in_jack(1).await;
    let outputjack = app.make_out_jack(0).await;

    let fut1 = async {
        loop {
            app.delay_millis(200).await;
            let value = inputjack.get_value();
            outputjack.set_value(2000);
            info!("VALUE, {:?}", value);
            
        }
    };

    fut1.await;
}
