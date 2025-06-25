use config::Config;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use midly::live::{LiveEvent, SystemRealtime};

use crate::{
    app::{App, ClockEvent},
    storage::ParamStore,
};

pub const CHANNELS: usize = 16;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> =
    Config::new("MIDI clock test", "Echo midi clock messages");

pub struct Params {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new([], app.app_id, app.start_channel);
    let params = Params {};

    let app_loop = async {
        loop {
            select(run(&app, &params), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, _params: &Params) {
    let midi = app.use_midi(0);
    let mut clock = app.use_clock();
    loop {
        match clock.wait_for_event(1).await {
            ClockEvent::Tick => {
                midi.send_msg(LiveEvent::Realtime(SystemRealtime::TimingClock))
                    .await;
            }
            ClockEvent::Start => {
                midi.send_msg(LiveEvent::Realtime(SystemRealtime::Start))
                    .await;
            }
            ClockEvent::Reset => {
                midi.send_msg(LiveEvent::Realtime(SystemRealtime::Stop))
                    .await;
            }
        }
    }
}
