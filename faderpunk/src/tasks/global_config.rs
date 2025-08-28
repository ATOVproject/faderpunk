use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use libfp::GlobalConfig;

use crate::QUANTIZER;

// Receivers: ext clock loops (3), internal clock loop (1), global config loop (1)
const GLOBAL_CONFIG_WATCH_SUBSCRIBERS: usize = 5;

pub static GLOBAL_CONFIG_WATCH: Watch<
    CriticalSectionRawMutex,
    GlobalConfig,
    GLOBAL_CONFIG_WATCH_SUBSCRIBERS,
> = Watch::new_with(GlobalConfig::new());

pub async fn start_global_config(spawner: &Spawner) {
    spawner.spawn(run_global_config()).unwrap();
}

#[embassy_executor::task]
async fn run_global_config() {
    let mut receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    loop {
        let old = GLOBAL_CONFIG_WATCH.try_get().unwrap();
        let config = receiver.changed().await;
        if config.quantizer_key != old.quantizer_key
            || config.quantizer_tonic != old.quantizer_tonic
        {
            // SAFETY: Not called re-entrantly
            unsafe {
                QUANTIZER
                    .get()
                    .lock_mut(|q| q.set_scale(config.quantizer_key, config.quantizer_tonic))
            }
        }
    }
}
