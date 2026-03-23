use embassy_executor::Spawner;
use libfp::Color;

use crate::app::Led;
use crate::events::{InputEvent, EVENT_PUBSUB};
use crate::tasks::leds::{set_led_overlay_mode, LedMode};

pub async fn start_input_handlers(spawner: &Spawner) {
    spawner.spawn(run_input_handlers()).unwrap();
}

#[embassy_executor::task]
async fn run_input_handlers() {
    let mut subscriber = EVENT_PUBSUB.subscriber().unwrap();
    loop {
        match subscriber.next_message_pure().await {
            InputEvent::LoadScene(scene) => {
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
            _ => {}
        }
    }
}
