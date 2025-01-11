use async_button::{Button, ButtonConfig, ButtonEvent};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join_array;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{
    PIN_23, PIN_24, PIN_25, PIN_28, PIN_29, PIN_30, PIN_31, PIN_32, PIN_33, PIN_34, PIN_35, PIN_36,
    PIN_37, PIN_38, PIN_4, PIN_5, PIN_6, PIN_7,
};
use embassy_time::Duration;
use embedded_hal::digital::InputPin;
use embedded_hal_async::digital::Wait;
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

pub async fn start_buttons(spawner: &Spawner, buttons: Buttons) {
    spawner.spawn(run_buttons(buttons)).unwrap();
}

async fn process_button<T: Wait + InputPin>(i: usize, mut button: Button<T>) {
    loop {
        match button.update().await {
            ButtonEvent::ShortPress { count } => {
                info!("Pressed button {} {} times", i, count);
            }
            ButtonEvent::LongPress => {
                info!("Long pressed button {}", i);
            }
        }
    }
}

#[embassy_executor::task]
async fn run_buttons(buttons: Buttons) {
    let config = ButtonConfig {
        debounce: Duration::from_millis(5),
        double_click: Duration::from_millis(0),
        ..ButtonConfig::default()
    };

    let button_futures = [
        process_button(0, Button::new(Input::new(buttons.0, Pull::Up), config)),
        process_button(1, Button::new(Input::new(buttons.1, Pull::Up), config)),
        process_button(2, Button::new(Input::new(buttons.2, Pull::Up), config)),
        process_button(3, Button::new(Input::new(buttons.3, Pull::Up), config)),
        process_button(4, Button::new(Input::new(buttons.4, Pull::Up), config)),
        process_button(5, Button::new(Input::new(buttons.5, Pull::Up), config)),
        process_button(6, Button::new(Input::new(buttons.6, Pull::Up), config)),
        process_button(7, Button::new(Input::new(buttons.7, Pull::Up), config)),
        process_button(8, Button::new(Input::new(buttons.8, Pull::Up), config)),
        process_button(9, Button::new(Input::new(buttons.9, Pull::Up), config)),
        process_button(10, Button::new(Input::new(buttons.10, Pull::Up), config)),
        process_button(11, Button::new(Input::new(buttons.11, Pull::Up), config)),
        process_button(12, Button::new(Input::new(buttons.12, Pull::Up), config)),
        process_button(13, Button::new(Input::new(buttons.13, Pull::Up), config)),
        process_button(14, Button::new(Input::new(buttons.14, Pull::Up), config)),
        process_button(15, Button::new(Input::new(buttons.15, Pull::Up), config)),
        process_button(16, Button::new(Input::new(buttons.16, Pull::Up), config)),
        process_button(17, Button::new(Input::new(buttons.17, Pull::Up), config)),
    ];

    join_array(button_futures).await;
}
