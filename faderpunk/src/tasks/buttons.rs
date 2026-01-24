use embassy_executor::Spawner;
use embassy_futures::join::{join, join_array};
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{
    PIN_23, PIN_24, PIN_25, PIN_28, PIN_29, PIN_30, PIN_31, PIN_32, PIN_33, PIN_34, PIN_35, PIN_36,
    PIN_37, PIN_38, PIN_4, PIN_5, PIN_6, PIN_7,
};
use embassy_rp::Peri;
use embassy_time::Timer;
use libfp::Color;
use portable_atomic::{AtomicBool, Ordering};

use crate::app::Led;
use crate::events::{EventPubSubPublisher, InputEvent, EVENT_PUBSUB};
use crate::tasks::clock::{TransportCmd, TRANSPORT_CMD_CHANNEL};

use super::leds::{set_led_overlay_mode, LedMode};

const LONG_PRESS_DURATION_MS: u64 = 500;

type Buttons = (
    Peri<'static, PIN_6>,
    Peri<'static, PIN_7>,
    Peri<'static, PIN_38>,
    Peri<'static, PIN_32>,
    Peri<'static, PIN_33>,
    Peri<'static, PIN_34>,
    Peri<'static, PIN_35>,
    Peri<'static, PIN_36>,
    Peri<'static, PIN_23>,
    Peri<'static, PIN_24>,
    Peri<'static, PIN_25>,
    Peri<'static, PIN_29>,
    Peri<'static, PIN_30>,
    Peri<'static, PIN_31>,
    Peri<'static, PIN_37>,
    Peri<'static, PIN_28>,
    Peri<'static, PIN_4>,
    Peri<'static, PIN_5>,
);

pub static BUTTON_PRESSED: [AtomicBool; 18] = [const { AtomicBool::new(false) }; 18];

pub async fn start_buttons(spawner: &Spawner, buttons: Buttons) {
    spawner.spawn(run_buttons(buttons)).unwrap();
}

#[inline(always)]
pub fn is_channel_button_pressed(channel: usize) -> bool {
    BUTTON_PRESSED[channel.clamp(0, 15)].load(Ordering::Relaxed)
}

#[inline(always)]
pub fn is_shift_button_pressed() -> bool {
    BUTTON_PRESSED[17].load(Ordering::Relaxed)
}

#[inline(always)]
pub fn is_scene_button_pressed() -> bool {
    BUTTON_PRESSED[16].load(Ordering::Relaxed)
}

// Process button using debounce and state synchronization logic
async fn process_button(i: usize, mut button: Input<'_>, event_publisher: &EventPubSubPublisher) {
    loop {
        if button.is_low() {
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
            button.wait_for_rising_edge().await;
            Timer::after_millis(10).await;

            if button.is_low() {
                continue;
            }
            BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
        }

        button.wait_for_falling_edge().await;

        Timer::after_millis(1).await;
        if button.is_high() {
            continue;
        }

        if BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            // Special mode when button 16 is pressed - handle scene load/save
            match select(
                button.wait_for_rising_edge(),
                Timer::after_millis(LONG_PRESS_DURATION_MS),
            )
            .await
            {
                Either::First(_) => {
                    // Short press - Load scene
                    set_led_overlay_mode(i, Led::Button, LedMode::Flash(Color::Green, Some(2)))
                        .await;
                    // TODO: experiment with using publish_immediate everywhere to prevent hanging
                    // subscribers
                    event_publisher
                        .publish(InputEvent::LoadScene(i as u8))
                        .await;
                }
                Either::Second(_) => {
                    // Long press - Save scene
                    set_led_overlay_mode(i, Led::Button, LedMode::Flash(Color::Red, Some(3))).await;
                    event_publisher
                        .publish(InputEvent::SaveScene(i as u8))
                        .await;

                    button.wait_for_rising_edge().await;
                }
            }
        } else {
            event_publisher.publish(InputEvent::ButtonDown(i)).await;
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);

            match select(
                button.wait_for_rising_edge(),
                Timer::after_millis(LONG_PRESS_DURATION_MS),
            )
            .await
            {
                Either::First(_) => {
                    event_publisher.publish(InputEvent::ButtonUp(i)).await;
                    BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
                }
                Either::Second(_) => {
                    if button.is_low() {
                        event_publisher
                            .publish(InputEvent::ButtonLongPress(i))
                            .await;

                        button.wait_for_rising_edge().await;
                    }

                    event_publisher.publish(InputEvent::ButtonUp(i)).await;
                    BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
                }
            }
        }

        Timer::after_millis(10).await;
    }
}

// Process modifier button using debounce and state synchronization logic
async fn process_modifier_button(i: usize, mut button: Input<'_>) {
    loop {
        if button.is_low() {
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
            button.wait_for_rising_edge().await;
            Timer::after_millis(1).await;

            if button.is_low() {
                continue;
            }

            BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
        }

        button.wait_for_falling_edge().await;

        Timer::after_millis(1).await;
        if button.is_high() {
            continue;
        }

        // Start clock if shift is pressed while scene is held
        if i == 17 && BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            TRANSPORT_CMD_CHANNEL.send(TransportCmd::Toggle).await;
        } else {
            // Do not register the button press
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
        }

        button.wait_for_rising_edge().await;

        Timer::after_millis(1).await;
        if button.is_low() {
            button.wait_for_rising_edge().await;
        }

        BUTTON_PRESSED[i].store(false, Ordering::Relaxed);

        Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
async fn run_buttons(buttons: Buttons) {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();
    let button_futs = [
        process_button(0, Input::new(buttons.0, Pull::Up), &event_publisher),
        process_button(1, Input::new(buttons.1, Pull::Up), &event_publisher),
        process_button(2, Input::new(buttons.2, Pull::Up), &event_publisher),
        process_button(3, Input::new(buttons.3, Pull::Up), &event_publisher),
        process_button(4, Input::new(buttons.4, Pull::Up), &event_publisher),
        process_button(5, Input::new(buttons.5, Pull::Up), &event_publisher),
        process_button(6, Input::new(buttons.6, Pull::Up), &event_publisher),
        process_button(7, Input::new(buttons.7, Pull::Up), &event_publisher),
        process_button(8, Input::new(buttons.8, Pull::Up), &event_publisher),
        process_button(9, Input::new(buttons.9, Pull::Up), &event_publisher),
        process_button(10, Input::new(buttons.10, Pull::Up), &event_publisher),
        process_button(11, Input::new(buttons.11, Pull::Up), &event_publisher),
        process_button(12, Input::new(buttons.12, Pull::Up), &event_publisher),
        process_button(13, Input::new(buttons.13, Pull::Up), &event_publisher),
        process_button(14, Input::new(buttons.14, Pull::Up), &event_publisher),
        process_button(15, Input::new(buttons.15, Pull::Up), &event_publisher),
    ];

    let modifier_futs = [
        process_modifier_button(16, Input::new(buttons.16, Pull::Up)),
        // Button 17 is pulled up in hardware
        process_modifier_button(17, Input::new(buttons.17, Pull::None)),
    ];

    join(join_array(button_futs), join_array(modifier_futs)).await;
}
