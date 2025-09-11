use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, watch::Watch};
use embassy_time::Timer;
use libfp::{AuxJackMode, Color, GlobalConfig, Key, Note, LED_BRIGHTNESS_RANGE};
use max11300::config::{ConfigMode0, ConfigMode3, Mode};
use portable_atomic::Ordering;

use crate::app::Led;
use crate::storage::store_global_config;
use crate::tasks::leds::{set_led_overlay_mode, LedMode, LED_BRIGHTNESS};
use crate::tasks::max::{MaxCmd, MAX_CHANNEL};
use crate::QUANTIZER;

// Receivers: ext clock loops (3), internal clock loop (1), clock ticker loop (1)
// global config loop (1), config storer (1)
const GLOBAL_CONFIG_WATCH_SUBSCRIBERS: usize = 7;

const LED_BRIGHTNESS_FADER: usize = 0;
const QUANTIZER_KEY_FADER: usize = 3;
const QUANTIZER_TONIC_FADER: usize = 4;
const INTERNAL_BPM_FADER: usize = 15;

pub static GLOBAL_CONFIG_WATCH: Watch<
    ThreadModeRawMutex,
    GlobalConfig,
    GLOBAL_CONFIG_WATCH_SUBSCRIBERS,
> = Watch::new_with(GlobalConfig::new());

pub fn get_global_config() -> GlobalConfig {
    // unwrap is fine here as it is always initialized (new_with)
    GLOBAL_CONFIG_WATCH.try_get().unwrap()
}

pub fn get_fader_value_from_config(chan: usize, config: &GlobalConfig) -> u16 {
    match chan {
        INTERNAL_BPM_FADER => (((config.clock.internal_bpm - 45.0) * 16.0) as u16).clamp(0, 4095),
        QUANTIZER_KEY_FADER => (config.quantizer.key as u16 * 256).clamp(0, 4095),
        QUANTIZER_TONIC_FADER => (config.quantizer.tonic as u16 * 342).clamp(0, 4095),
        LED_BRIGHTNESS_FADER => ((config.led_brightness as u16 - 55) * 20).clamp(0, 4095),
        _ => 0,
    }
}

pub fn set_global_config_via_chan(chan: usize, val: u16) {
    let global_config_sender = GLOBAL_CONFIG_WATCH.sender();
    match chan {
        LED_BRIGHTNESS_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_brightness = (LED_BRIGHTNESS_RANGE.start as u16 + (val / 20)).clamp(
                        LED_BRIGHTNESS_RANGE.start as u16,
                        LED_BRIGHTNESS_RANGE.end as u16,
                    ) as u8;
                    if config.led_brightness != new_brightness {
                        config.led_brightness = new_brightness;
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
                    if config.quantizer.key != new_key {
                        config.quantizer.key = new_key;
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
                    if config.quantizer.tonic != new_tonic {
                        config.quantizer.tonic = new_tonic;
                        return true;
                    }
                }
                false
            });
        }
        INTERNAL_BPM_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_bpm = (45.0 + val as f32 / 16.0).clamp(0.0, 300.0);
                    if config.clock.internal_bpm != new_bpm {
                        config.clock.internal_bpm = new_bpm;
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
    quantizer.set_scale(old.quantizer.key, old.quantizer.tonic);
    drop(quantizer);

    for (i, aux_jack) in old.aux.iter().enumerate() {
        if let AuxJackMode::ClockOut(_) = aux_jack {
            info!("SETTING CLOCK OUT TO JACK {}", 17 + i);
            MAX_CHANNEL
                .send((
                    17 + i,
                    MaxCmd::ConfigurePort(Mode::Mode3(ConfigMode3), Some(2048)),
                ))
                .await;
        }
    }

    // Clock has a subscriber to the config (so no need to Initialize it here)
    // TODO: Shall we blink an LED in the rythm of the clock for a couple of seconds when it was
    // changed?

    // TODO: Actually find good colors or effects to signal the changes to global config
    loop {
        let config = receiver.changed().await;
        if config.quantizer.key != old.quantizer.key
            || config.quantizer.tonic != old.quantizer.tonic
        {
            let mut quantizer = QUANTIZER.get().lock().await;
            quantizer.set_scale(config.quantizer.key, config.quantizer.tonic);
            if config.quantizer.key != old.quantizer.key {
                let color = Color::from(config.quantizer.key as usize);
                set_led_overlay_mode(
                    QUANTIZER_KEY_FADER,
                    Led::Button,
                    LedMode::StaticFade(color, 2000),
                )
                .await;
            } else {
                let color = Color::from(config.quantizer.tonic as usize);
                set_led_overlay_mode(
                    QUANTIZER_TONIC_FADER,
                    Led::Button,
                    LedMode::StaticFade(color, 2000),
                )
                .await;
            }
        }
        if config.led_brightness != old.led_brightness {
            LED_BRIGHTNESS.store(config.led_brightness, Ordering::Relaxed);
        }

        for (i, (new_aux, old_aux)) in config.aux.iter().zip(old.aux.iter()).enumerate() {
            if new_aux != old_aux {
                if let AuxJackMode::ClockOut(_) = new_aux {
                    info!("SETTING CLOCK OUT TO JACK {}", 17 + i);
                    MAX_CHANNEL
                        .send((
                            17 + i,
                            MaxCmd::ConfigurePort(Mode::Mode3(ConfigMode3), Some(2048)),
                        ))
                        .await;
                } else {
                    MAX_CHANNEL
                        .send((
                            17 + i,
                            MaxCmd::ConfigurePort(Mode::Mode0(ConfigMode0), None),
                        ))
                        .await;
                }
            }
        }
        old = config;
    }
}
