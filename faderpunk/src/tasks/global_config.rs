use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, watch::Watch};
use embassy_time::Timer;
use libfp::{AuxJackMode, GlobalConfig, Key, Note, LED_BRIGHTNESS_RANGE};
use max11300::config::{ConfigMode0, ConfigMode3, Mode, Port};
use portable_atomic::Ordering;

use crate::layout::FORCE_RESPAWN_SIGNAL;
use crate::storage::store_global_config;
use crate::tasks::buttons::is_scene_button_pressed;
use crate::tasks::input_handlers::{show_config_top_leds, show_scale_keyboard};
use crate::tasks::leds::LED_BRIGHTNESS;
use crate::tasks::max::{MaxCmd, MAX_CHANNEL};
use crate::QUANTIZER;

// Receivers: unified clock engine (1), clock gatekeeper (1)
// global config loop (1), config storer (1), MIDI loops (2)
const GLOBAL_CONFIG_WATCH_SUBSCRIBERS: usize = 6;

const LED_BRIGHTNESS_FADER: usize = 0;
const QUANTIZER_KEY_FADER: usize = 3;
const QUANTIZER_TONIC_FADER: usize = 4;
const SWING_FADER: usize = 14;
const INTERNAL_BPM_FADER: usize = 15;

fn val_to_swing(val: u16) -> i8 {
    (((val as i32 * 70) / 4095) - 35).clamp(-35, 35) as i8
}

fn swing_to_val(swing: i8) -> u16 {
    (((swing.clamp(-35, 35) as i32 + 35) * 4095) / 70).clamp(0, 4095) as u16
}

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
        SWING_FADER => swing_to_val(config.clock.swing_amount),
        QUANTIZER_KEY_FADER => (config.quantizer.key as u16 * 256).clamp(0, 4095),
        QUANTIZER_TONIC_FADER => (config.quantizer.tonic as u16 * 342).clamp(0, 4095),
        LED_BRIGHTNESS_FADER => {
            let brightness_range = (LED_BRIGHTNESS_RANGE.end - LED_BRIGHTNESS_RANGE.start) as u32;
            let norm_brightness =
                (config.led_brightness as u32).saturating_sub(LED_BRIGHTNESS_RANGE.start as u32);
            // Reverse the curve by taking the square root
            let target_val = (norm_brightness * 4095) / brightness_range;
            (target_val * 4095).isqrt().clamp(0, 4095) as u16
        }
        _ => 0,
    }
}

pub fn set_global_config_via_chan(chan: usize, val: u16) {
    let global_config_sender = GLOBAL_CONFIG_WATCH.sender();
    match chan {
        LED_BRIGHTNESS_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    // Apply an exponential curve by squaring the value
                    let val_u32 = val as u32;
                    let curved_val = (val_u32 * val_u32) / 4095;
                    let brightness_range =
                        (LED_BRIGHTNESS_RANGE.end - LED_BRIGHTNESS_RANGE.start) as u32;
                    let new_brightness = (LED_BRIGHTNESS_RANGE.start as u32
                        + (curved_val * brightness_range) / 4095)
                        .clamp(
                            LED_BRIGHTNESS_RANGE.start as u32,
                            LED_BRIGHTNESS_RANGE.end as u32,
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
        SWING_FADER => {
            global_config_sender.send_if_modified(|c| {
                if let Some(config) = c {
                    let new_swing = val_to_swing(val);
                    if config.clock.swing_amount != new_swing {
                        config.clock.swing_amount = new_swing;
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

async fn set_aux_config(aux_port: usize, aux_jack_mode: &AuxJackMode) {
    let port = Port::try_from(17 + aux_port).unwrap();
    match aux_jack_mode {
        AuxJackMode::ClockOut(_) | AuxJackMode::ResetOut => {
            MAX_CHANNEL
                .send(MaxCmd::ConfigurePort {
                    port,
                    mode: Mode::Mode3(ConfigMode3),
                    gpo_level: Some(2048),
                })
                .await;
        }
        AuxJackMode::None => {
            MAX_CHANNEL
                .send(MaxCmd::ConfigurePort {
                    port,
                    mode: Mode::Mode0(ConfigMode0),
                    gpo_level: None,
                })
                .await;
        }
    }
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
        set_aux_config(i, aux_jack).await;
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
            if is_scene_button_pressed() {
                show_scale_keyboard(config.quantizer.key, config.quantizer.tonic).await;
                show_config_top_leds(&config).await;
            }
        }
        if config.clock.swing_amount != old.clock.swing_amount && is_scene_button_pressed() {
            show_config_top_leds(&config).await;
        }
        if config.led_brightness != old.led_brightness {
            LED_BRIGHTNESS.store(config.led_brightness, Ordering::Relaxed);
        }

        for (i, (new_aux, old_aux)) in config.aux.iter().zip(old.aux.iter()).enumerate() {
            if new_aux != old_aux {
                set_aux_config(i, new_aux).await;
            }
        }

        // Re-spawn all apps if takeover_mode changed
        if config.takeover_mode != old.takeover_mode {
            // Signal the layout manager to force respawn all apps
            FORCE_RESPAWN_SIGNAL.signal(());
        }

        old = config;
    }
}
