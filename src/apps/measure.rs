use defmt::info;

use crate::app::App;
use embassy_futures::join::join;
pub const CHANNELS: usize = 1;

pub async fn run(app: App<CHANNELS>) {
    let jacks = app.make_in_jack(0, crate::app::Range::_0_10V).await;
    let fut1 = async {
        loop {
            let value = jacks.get_value();
            info!("VALUE, {:?}", value);
            app.delay_millis(10).await;
            app.midi_send_cc(0, 10).await;
        }
    };

    let fut2 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
   
            info!("button press");
        }
    };

    join(fut1, fut2).await;
}
