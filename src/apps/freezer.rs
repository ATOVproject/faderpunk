use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let outputjack = app.make_out_jack(0).await;

    let fut1 = async {
        loop {
            app.delay_millis(10).await;
            app.midi_send_cc(1, 0).await;
            //info!("Send midi on channel {}", app.channels[0]);
  
        }
    };

    fut1.await;
}
