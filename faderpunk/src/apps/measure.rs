use config::Config;
use defmt::info;

use crate::app::App;

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Measure", "Test app to measure port voltages");

pub async fn run(app: App<CHANNELS>) {
    let input = app.make_out_jack(0, crate::app::Range::_0_10V).await;
    let mut die = app.use_die();
    let mut value = 0;
    let fut1 = async {
        loop {
            input.set_value(4095);
            value += 1024;
            value = value % 4096;
            info!("VALUE, {:?}", value);
            app.delay_millis(6000).await;
        }
    };

    fut1.await;
}
