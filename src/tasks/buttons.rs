use crate::{XTxMsg, XTxSender};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join_array;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::{
    PIN_23, PIN_24, PIN_25, PIN_28, PIN_29, PIN_30, PIN_31, PIN_32, PIN_33, PIN_34, PIN_35, PIN_36,
    PIN_37, PIN_38, PIN_4, PIN_5, PIN_6, PIN_7,
};
use portable_atomic::{AtomicBool, Ordering};

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

pub async fn start_buttons(spawner: &Spawner, buttons: Buttons, sender: XTxSender) {
    spawner.spawn(run_buttons(buttons, sender)).unwrap();
}

async fn process_button(i: usize, mut button: Input<'_>, sender: &XTxSender) {
    loop {
        button.wait_for_falling_edge().await;
        if i <= 15 {
            // Only process the channel buttons here
            sender.send((i, XTxMsg::ButtonDown)).await;
        } else {
            // TODO: Do we want to handle the special buttons somehow?
        }
        BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
        button.wait_for_rising_edge().await;
        BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
    }
}

#[embassy_executor::task]
async fn run_buttons(buttons: Buttons, sender: XTxSender) {
    let button_futures = [
        process_button(0, Input::new(buttons.0, Pull::Up), &sender),
        process_button(1, Input::new(buttons.1, Pull::Up), &sender),
        process_button(2, Input::new(buttons.2, Pull::Up), &sender),
        process_button(3, Input::new(buttons.3, Pull::Up), &sender),
        process_button(4, Input::new(buttons.4, Pull::Up), &sender),
        process_button(5, Input::new(buttons.5, Pull::Up), &sender),
        process_button(6, Input::new(buttons.6, Pull::Up), &sender),
        process_button(7, Input::new(buttons.7, Pull::Up), &sender),
        process_button(8, Input::new(buttons.8, Pull::Up), &sender),
        process_button(9, Input::new(buttons.9, Pull::Up), &sender),
        process_button(10, Input::new(buttons.10, Pull::Up), &sender),
        process_button(11, Input::new(buttons.11, Pull::Up), &sender),
        process_button(12, Input::new(buttons.12, Pull::Up), &sender),
        process_button(13, Input::new(buttons.13, Pull::Up), &sender),
        process_button(14, Input::new(buttons.14, Pull::Up), &sender),
        process_button(15, Input::new(buttons.15, Pull::Up), &sender),
        process_button(16, Input::new(buttons.16, Pull::Up), &sender),
        // Button 17 is pulled up in hardware
        process_button(17, Input::new(buttons.17, Pull::None), &sender),
    ];

    join_array(button_futures).await;
}
