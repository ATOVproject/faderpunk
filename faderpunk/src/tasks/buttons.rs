use embassy_executor::Spawner;
use embassy_futures::join::{join, join_array};
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{
    PIN_23, PIN_24, PIN_25, PIN_28, PIN_29, PIN_30, PIN_31, PIN_32, PIN_33, PIN_34, PIN_35, PIN_36,
    PIN_37, PIN_38, PIN_4, PIN_5, PIN_6, PIN_7,
};
use embassy_time::Timer;
use portable_atomic::{AtomicBool, Ordering};
use smart_leds::colors::{GREEN, RED};

use crate::app::Led;
use crate::events::{EventPubSubPublisher, InputEvent, EVENT_PUBSUB};

use super::leds::{LedMode, LedMsg, LED_CHANNEL};

type Buttons = (
    PIN_6,
    PIN_7,
    PIN_38,
    PIN_32,
    PIN_33,
    PIN_34,
    PIN_35,
    PIN_36,
    PIN_23,
    PIN_24,
    PIN_25,
    PIN_29,
    PIN_30,
    PIN_31,
    PIN_37,
    PIN_28,
    PIN_4,
    PIN_5,
);

pub static BUTTON_PRESSED: [AtomicBool; 18] = [const { AtomicBool::new(false) }; 18];

pub async fn start_buttons(spawner: &Spawner, buttons: Buttons) {
    spawner.spawn(run_buttons(buttons)).unwrap();
}

async fn process_button(i: usize, mut button: Input<'_>, event_publisher: &EventPubSubPublisher) {
    loop {
        button.wait_for_falling_edge().await;
        if BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            // Debounce a bit (bounces are all in sub 1ms)
            Timer::after_millis(1).await;
            match select(button.wait_for_rising_edge(), Timer::after_millis(1500)).await {
                Either::First(_) => {
                    LED_CHANNEL
                        .send(LedMsg::SetOverlay(i, Led::Button, LedMode::Flash(GREEN, 2)))
                        .await;
                    event_publisher
                        .publish(InputEvent::LoadScene(i as u8))
                        .await;
                }
                Either::Second(_) => {
                    LED_CHANNEL
                        .send(LedMsg::SetOverlay(i, Led::Button, LedMode::Flash(RED, 3)))
                        .await;
                    event_publisher
                        .publish(InputEvent::SaveScene(i as u8))
                        .await;
                    button.wait_for_rising_edge().await;
                }
            }
        } else {
            event_publisher.publish(InputEvent::ButtonDown(i)).await;
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
            // Debounce a bit (bounces are all in sub 1ms)
            Timer::after_millis(1).await;
            button.wait_for_rising_edge().await;
            event_publisher.publish(InputEvent::ButtonUp(i)).await;
            BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
        }
        // Release debounce
        Timer::after_millis(1).await;
    }
}

async fn process_modifier_button(i: usize, mut button: Input<'_>) {
    loop {
        button.wait_for_falling_edge().await;
        BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
        Timer::after_millis(1).await;
        button.wait_for_rising_edge().await;
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
