use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use embassy_time::Timer;
use libfp::{GlobalConfig, Key, Note};
use portable_atomic::Ordering;
use smart_leds::colors::RED;

use crate::app::Led;
use crate::storage::store_global_config;
use crate::tasks::leds::{set_led_overlay_mode, LedMode, LED_BRIGHTNESS};
use crate::QUANTIZER;

// Receivers: ext clock loops (3), internal clock loop (1), global config loop (1), config storer
// (1)
const GLOBAL_CONFIG_WATCH_SUBSCRIBERS: usize = 6;

const INTERNAL_BPM_FADER: usize = 0;
const QUANTIZER_KEY_FADER: usize = 3;
const QUANTIZER_TONIC_FADER: usize = 4;
const LED_BRIGHTNESS_FADER: usize = 15;

const MIN_LED_BRIGHTNESS: u8 = 85;

pub static GLOBAL_CONFIG_WATCH: Watch<
    CriticalSectionRawMutex,
    GlobalConfig,
    GLOBAL_CONFIG_WATCH_SUBSCRIBERS,
> = Watch::new_with(GlobalConfig::new());

pub fn get_global_config() -> GlobalConfig {
    // unwrap is fine here as it is always initialized (new_with)
    GLOBAL_CONFIG_WATCH.try_get().unwrap()
}

pub fn get_fader_value_from_config(chan: usize, config: &GlobalConfig) -> u16 {
    match chan {
        INTERNAL_BPM_FADER => (((config.internal_bpm - 45.0) * 16.0) as u16).clamp(0, 4095),
        QUANTIZER_KEY_FADER => (config.quantizer_key as u16 * 256).clamp(0, 4095),
        QUANTIZER_TONIC_FADER => (config.quantizer_tonic as u16 * 342).clamp(0, 4095),
        LED_BRIGHTNESS_FADER => ((config.led_brightness as u16 - 55) * 20).clamp(0, 4095),
        _ => 0,
    }
}

pub fn set_global_config_via_chan(chan: usize, val: u16) {
    let global_config_sender = GLOBAL_CONFIG_WATCH.sender();
    match chan {
        INTERNAL_BPM_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_bpm = (45.0 + val as f32 / 16.0).clamp(0.0, 300.0);
                    if config.internal_bpm != new_bpm {
                        config.internal_bpm = new_bpm;
                        return true;
                    }
                }
                false
            });
        }
        QUANTIZER_KEY_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_key: Key = unsafe { core::mem::transmute((val / 256) as u8) };
                    if config.quantizer_key != new_key {
                        config.quantizer_key = new_key;
                        return true;
                    }
                }
                false
            });
        }
        QUANTIZER_TONIC_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_tonic: Note = unsafe { core::mem::transmute((val / 342) as u8) };
                    if config.quantizer_tonic != new_tonic {
                        config.quantizer_tonic = new_tonic;
                        return true;
                    }
                }
                false
            });
        }
        LED_BRIGHTNESS_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_brightness = (MIN_LED_BRIGHTNESS as u16 + (val / 20))
                        .clamp(MIN_LED_BRIGHTNESS as u16, 255)
                        as u8;
                    if config.led_brightness != new_brightness {
                        config.led_brightness = new_brightness;
                        return true;
                    }
                }
                false
            });
        }
        _ => {}
    }
}

pub async fn start_global_config(spawner: &Spawner) {
    spawner.spawn(config_storer()).unwrap();
    spawner.spawn(global_config_change()).unwrap();
}

#[embassy_executor::task]
async fn config_storer() {
    let mut receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    loop {
        let mut config = receiver.changed().await;

        loop {
            match select(Timer::after_secs(1), receiver.changed()).await {
                Either::First(_) => {
                    store_global_config(&config).await;
                    break;
                }
                Either::Second(new_config) => {
                    config = new_config;
                }
            }
        }
    }
}

#[embassy_executor::task]
async fn global_config_change() {
    let mut receiver = GLOBAL_CONFIG_WATCH.receiver().unwrap();
    let mut old = get_global_config();

    // Initialize leds with loaded config
    LED_BRIGHTNESS.store(old.led_brightness, Ordering::Relaxed);

    // Initialize quantizer with loaded config
    let mut quantizer = QUANTIZER.get().lock().await;
    quantizer.set_scale(old.quantizer_key, old.quantizer_tonic);
    drop(quantizer);

    // Clock has a subscriber to the config (so no need to Initialize it here)

    // TODO: Actually find good colors or effects to signal the changes to global config
    loop {
        let config = receiver.changed().await;
        if config.quantizer_key != old.quantizer_key
            || config.quantizer_tonic != old.quantizer_tonic
        {
            let mut quantizer = QUANTIZER.get().lock().await;
            quantizer.set_scale(config.quantizer_key, config.quantizer_tonic);
            if config.quantizer_key != old.quantizer_key {
                set_led_overlay_mode(
                    QUANTIZER_KEY_FADER,
                    Led::Button,
                    LedMode::StaticFade(RED, 5000),
                )
                .await;
            } else {
                set_led_overlay_mode(
                    QUANTIZER_TONIC_FADER,
                    Led::Button,
                    LedMode::StaticFade(RED, 5000),
                )
                .await;
            }
        }
        if config.led_brightness != old.led_brightness {
            LED_BRIGHTNESS.store(config.led_brightness, Ordering::Relaxed);
        }
        old = config;
    }
}
