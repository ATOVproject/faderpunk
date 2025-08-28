use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use libfp::GlobalConfig;
use portable_atomic::Ordering;

use crate::tasks::clock::{ClockCmd, CLOCK_CMD_CHANNEL};
use crate::tasks::leds::LED_BRIGHTNESS;
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
    let clock_sender = CLOCK_CMD_CHANNEL.sender();
    loop {
        let old = GLOBAL_CONFIG_WATCH.try_get().unwrap();
        let config = receiver.changed().await;
        if config.quantizer_key != old.quantizer_key
            || config.quantizer_tonic != old.quantizer_tonic
        {
            let mut quantizer = QUANTIZER.get().lock().await;
            quantizer.set_scale(config.quantizer_key, config.quantizer_tonic);
        }
        if config.led_brightness != old.led_brightness {
            LED_BRIGHTNESS.store(config.led_brightness, Ordering::Relaxed);
        }
        if config.internal_bpm != old.internal_bpm {
            clock_sender
                .send(ClockCmd::SetBpm(config.internal_bpm))
                .await;
        }
    }
}
