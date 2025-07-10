use crate::app::{App, Range};
use config::Config;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> =
    Config::new("Calibrator test", "Just putting out some values");

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let output = app.make_out_jack(0, Range::_0_10V).await;
    let fader = app.use_faders();

    let fut1 = async {
        loop {
            app.delay_secs(1).await;
            let fader_val = fader.get_values()[0];

            // Lock into 11 discrete values: 0V (0), 1V, 2V, ..., 10V (4095)
            // Each voltage step corresponds to 409.5 ADC units (4095/10)
            let locked_val = match fader_val {
                0..=205 => 0,        // 0V
                206..=614 => 411,    // 1V
                615..=1023 => 819,   // 2V
                1024..=1432 => 1229, // 3V
                1433..=1841 => 1638, // 4V
                1842..=2250 => 2048, // 5V
                2251..=2659 => 2457, // 6V
                2660..=3068 => 2867, // 7V
                3069..=3477 => 3276, // 8V
                3478..=3886 => 3686, // 9V
                3887..=4095 => 4095, // 10V
                _ => 4095,
            };

            defmt::info!("FADER: {} -> LOCKED: {}", fader_val, locked_val);
            output.set_value(locked_val);
        }
    };

    fut1.await;
}
