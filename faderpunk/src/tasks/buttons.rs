use embassy_executor::Spawner;
use embassy_futures::join::{join, join_array};
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{
    PIN_23, PIN_24, PIN_25, PIN_28, PIN_29, PIN_30, PIN_31, PIN_32, PIN_33, PIN_34, PIN_35, PIN_36,
    PIN_37, PIN_38, PIN_4, PIN_5, PIN_6, PIN_7,
};
use embassy_time::{with_timeout, Duration, Timer};
use portable_atomic::{AtomicBool, Ordering};

use crate::scene::{get_scene, set_scene};
use crate::storage::{AppStorageCmd, AppStoragePublisher, APP_STORAGE_CMD_PUBSUB};
use crate::{EventPubSubPublisher, HardwareEvent, EVENT_PUBSUB};

use {defmt_rtt as _, panic_probe as _};

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

async fn process_button(
    i: usize,
    mut button: Input<'_>,
    app_storage_publisher: &AppStoragePublisher,
    event_publisher: &EventPubSubPublisher,
) {
    loop {
        button.wait_for_falling_edge().await;
        if BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            // Debounce a bit (bounces are all in sub 1ms)
            Timer::after_millis(1).await;
            if with_timeout(Duration::from_secs(1), button.wait_for_rising_edge())
                .await
                .is_ok()
            {
                let current_scene = get_scene();
                if (i as u8) != current_scene {
                    set_scene(i as u8);
                    app_storage_publisher
                        .publish(AppStorageCmd::LoadScene)
                        .await;
                }
            } else {
                app_storage_publisher
                    .publish(AppStorageCmd::SaveScene { scene: i as u8 })
                    .await;
                button.wait_for_rising_edge().await;
            }
        } else {
            event_publisher.publish(HardwareEvent::ButtonDown(i)).await;
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
            // Debounce a bit (bounces are all in sub 1ms)
            Timer::after_millis(1).await;
            button.wait_for_rising_edge().await;
            event_publisher.publish(HardwareEvent::ButtonUp(i)).await;
            BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
            // Debounce a bit more (bounces are all in sub 1ms)
            Timer::after_millis(1).await;
        }
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
    let app_storage_publisher = APP_STORAGE_CMD_PUBSUB.publisher().unwrap();
    let button_futs = [
        process_button(
            0,
            Input::new(buttons.0, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            1,
            Input::new(buttons.1, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            2,
            Input::new(buttons.2, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            3,
            Input::new(buttons.3, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            4,
            Input::new(buttons.4, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            5,
            Input::new(buttons.5, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            6,
            Input::new(buttons.6, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            7,
            Input::new(buttons.7, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            8,
            Input::new(buttons.8, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            9,
            Input::new(buttons.9, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            10,
            Input::new(buttons.10, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            11,
            Input::new(buttons.11, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            12,
            Input::new(buttons.12, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            13,
            Input::new(buttons.13, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            14,
            Input::new(buttons.14, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
        process_button(
            15,
            Input::new(buttons.15, Pull::Up),
            &app_storage_publisher,
            &event_publisher,
        ),
    ];

    let modifier_futs = [
        process_modifier_button(16, Input::new(buttons.16, Pull::Up)),
        // Button 17 is pulled up in hardware
        process_modifier_button(17, Input::new(buttons.17, Pull::None)),
    ];

    join(join_array(button_futs), join_array(modifier_futs)).await;
}
