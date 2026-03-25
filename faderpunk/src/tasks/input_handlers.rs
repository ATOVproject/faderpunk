use embassy_executor::Spawner;
use libfp::{Brightness, Color, Key, Note};
use portable_atomic::{AtomicU8, Ordering};

use crate::app::Led;
use crate::events::{InputEvent, EVENT_PUBSUB};
use crate::tasks::global_config::get_global_config;
use crate::tasks::leds::{clear_led_overlay, set_led_overlay_mode, LedMode};

static LAST_SCENE: AtomicU8 = AtomicU8::new(u8::MAX);

const SCALE_LED_FIRST_CHANNEL: usize = 3;
const SCALE_LED_LAST_CHANNEL: usize = SCALE_LED_FIRST_CHANNEL + SCALE_LED_COUNT;
const SCALE_LED_COUNT: usize = 12;
const NUM_CHANNELS: usize = 16;

/// Piano black-key pattern: C=white, C#=black, D=white, D#=black, E=white,
/// F=white, F#=black, G=white, G#=black, A=white, A#=black, B=white
const IS_BLACK_KEY: [bool; 12] = [
    false, true, false, true, false, false, true, false, true, false, true, false,
];

pub async fn start_input_handlers(spawner: &Spawner) {
    spawner.spawn(run_input_handlers()).unwrap();
}

#[embassy_executor::task]
async fn run_input_handlers() {
    let mut subscriber = EVENT_PUBSUB.subscriber().unwrap();
    loop {
        match subscriber.next_message_pure().await {
            InputEvent::LoadScene(scene) => {
                LAST_SCENE.store(scene, Ordering::Relaxed);
                set_led_overlay_mode(
                    scene as usize,
                    Led::Button,
                    LedMode::Flash(Color::Green, Some(2)),
                )
                .await;
            }
            InputEvent::SaveScene(scene) => {
                set_led_overlay_mode(
                    scene as usize,
                    Led::Button,
                    LedMode::Flash(Color::Red, Some(3)),
                )
                .await;
            }
            InputEvent::SceneButtonDown => {
                // Suppress all app LEDs to create a "settings page"
                for i in 0..NUM_CHANNELS {
                    set_led_overlay_mode(
                        i,
                        Led::Top,
                        LedMode::Static(Color::White, Brightness::Off),
                    )
                    .await;
                    set_led_overlay_mode(
                        i,
                        Led::Button,
                        LedMode::Static(Color::White, Brightness::Off),
                    )
                    .await;
                }

                let config = get_global_config();
                show_scale_keyboard(config.quantizer.key, config.quantizer.tonic).await;

                let last = LAST_SCENE.load(Ordering::Relaxed);
                if last < NUM_CHANNELS as u8 {
                    set_led_overlay_mode(
                        last as usize,
                        Led::Button,
                        LedMode::Static(Color::Green, Brightness::Mid),
                    )
                    .await;
                }
            }
            InputEvent::SceneButtonUp => {
                for i in 0..NUM_CHANNELS {
                    clear_led_overlay(i, Led::Top).await;
                    clear_led_overlay(i, Led::Bottom).await;
                    clear_led_overlay(i, Led::Button).await;
                }
            }
            _ => {}
        }
    }
}

pub async fn show_scale_keyboard(key: Key, tonic: Note) {
    let mask = key.as_u16_key();
    let tonic_offset = tonic as usize;

    for (semitone, &black_key) in IS_BLACK_KEY.iter().enumerate() {
        // The mask is MSB=C (bit 11) to LSB=B (bit 0), offset by tonic
        let note_index = (semitone + tonic_offset) % 12;
        let in_scale = (mask >> (11 - note_index)) & 1 != 0;

        let color = if black_key {
            Color::Yellow
        } else {
            Color::White
        };

        let brightness = if semitone == tonic_offset {
            Brightness::High
        } else if in_scale {
            Brightness::Mid
        } else {
            Brightness::Low
        };

        let mode = LedMode::Static(color, brightness);

        set_led_overlay_mode(SCALE_LED_FIRST_CHANNEL + semitone, Led::Bottom, mode).await;
    }

    // Mute channels outside the scale keyboard
    for ch in 0..SCALE_LED_FIRST_CHANNEL {
        set_led_overlay_mode(
            ch,
            Led::Bottom,
            LedMode::Static(Color::White, Brightness::Off),
        )
        .await;
    }
    for ch in SCALE_LED_LAST_CHANNEL..NUM_CHANNELS {
        set_led_overlay_mode(
            ch,
            Led::Bottom,
            LedMode::Static(Color::White, Brightness::Off),
        )
        .await;
    }
}
