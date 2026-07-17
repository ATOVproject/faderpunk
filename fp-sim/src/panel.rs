//! Virtual panel input: buttons and faders.
//!
//! The UI (or any other frontend) writes raw physical state — fader positions
//! into [`SIM_FADER_POS`] and button press/release transitions via
//! [`set_button`] — and the tasks here translate it into the same
//! `InputEvent`s and shared state the firmware produces, mirroring
//! `faderpunk/src/tasks/buttons.rs` (long-press, scene-hold load/save,
//! shift+scene transport toggle) and the fader half of
//! `faderpunk/src/tasks/max.rs` (`AnalogLatch` layers: main vs. global
//! settings while the scene button is held). GPIO debounce is omitted — UI
//! events are already clean.

use embassy_futures::join::{join, join_array};
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use portable_atomic::{AtomicU16, Ordering};

use libfp::latch::{AnalogLatch, LatchLayer};

use fp_core::events::{EventPubSubPublisher, InputEvent, EVENT_PUBSUB};
use fp_core::tasks::buttons::{is_scene_button_pressed, BUTTON_PRESSED};
use fp_core::tasks::clock::{TransportCmd, TRANSPORT_CMD_CHANNEL};
use fp_core::tasks::global_config::{
    get_fader_value_from_config, get_global_config, set_global_config_via_chan,
};
use fp_core::tasks::max::MAX_VALUES_FADER;

const LONG_PRESS_DURATION_MS: u64 = 500;

/// Physical fader positions as set by the UI (0..=4095). The latch task reads
/// these; apps read `MAX_VALUES_FADER`, which only follows once latched.
pub static SIM_FADER_POS: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];

/// Raw press (`true`) / release (`false`) transitions per button.
/// 0..=15 channel buttons, 16 scene, 17 shift.
static BUTTON_RAW: [Channel<CriticalSectionRawMutex, bool, 8>; 18] = [const { Channel::new() }; 18];

/// Feeds a raw button transition from the frontend. Safe to call from any
/// thread; drops the transition if the queue is full (UI can't outrun 8
/// pending transitions in practice).
pub fn set_button(i: usize, pressed: bool) {
    let _ = BUTTON_RAW[i].try_send(pressed);
}

async fn wait_release(i: usize) {
    while BUTTON_RAW[i].receive().await {}
}

async fn wait_press(i: usize) {
    while !BUTTON_RAW[i].receive().await {}
}

/// Mirror of the firmware's `process_button` for channel buttons 0..=15.
async fn process_button(i: usize, event_publisher: &EventPubSubPublisher) {
    loop {
        wait_press(i).await;

        if BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            // Scene button held - short press loads, long press saves
            match select(wait_release(i), Timer::after_millis(LONG_PRESS_DURATION_MS)).await {
                Either::First(_) => {
                    event_publisher
                        .publish(InputEvent::LoadSceneFromButton(i as u8))
                        .await;
                }
                Either::Second(_) => {
                    event_publisher
                        .publish(InputEvent::SaveScene(i as u8))
                        .await;

                    wait_release(i).await;
                }
            }
        } else {
            event_publisher.publish(InputEvent::ButtonDown(i)).await;
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);

            match select(wait_release(i), Timer::after_millis(LONG_PRESS_DURATION_MS)).await {
                Either::First(_) => {
                    event_publisher.publish(InputEvent::ButtonUp(i)).await;
                    BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
                }
                Either::Second(_) => {
                    event_publisher
                        .publish(InputEvent::ButtonLongPress(i))
                        .await;

                    wait_release(i).await;

                    event_publisher.publish(InputEvent::ButtonUp(i)).await;
                    BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Mirror of the firmware's `process_modifier_button` for scene (16) and
/// shift (17).
async fn process_modifier_button(i: usize, event_publisher: &EventPubSubPublisher) {
    let (down_event, up_event) = match i {
        16 => (InputEvent::SceneButtonDown, InputEvent::SceneButtonUp),
        17 => (InputEvent::ShiftButtonDown, InputEvent::ShiftButtonUp),
        _ => unreachable!("only called for modifier buttons 16 and 17"),
    };

    loop {
        wait_press(i).await;

        // Toggle the clock if shift is pressed while scene is held
        if i == 17 && BUTTON_PRESSED[16].load(Ordering::Relaxed) {
            TRANSPORT_CMD_CHANNEL.send(TransportCmd::Toggle).await;
        } else {
            BUTTON_PRESSED[i].store(true, Ordering::Relaxed);
            event_publisher.publish(down_event.clone()).await;
        }

        wait_release(i).await;

        BUTTON_PRESSED[i].store(false, Ordering::Relaxed);
        event_publisher.publish(up_event.clone()).await;
    }
}

#[embassy_executor::task]
pub async fn run_buttons() {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let button_futs: [_; 16] = core::array::from_fn(|i| process_button(i, &event_publisher));
    let modifier_futs = [
        process_modifier_button(16, &event_publisher),
        process_modifier_button(17, &event_publisher),
    ];

    join(join_array(button_futs), join_array(modifier_futs)).await;
}

/// Mirror of the fader half of the firmware's `read_fader`: runs the
/// `AnalogLatch` per channel over the UI-provided positions, switching to the
/// global-settings layer while the scene button is held.
#[embassy_executor::task]
pub async fn run_faders() {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();
    let global_config = get_global_config();

    let mut main_fader_values: [u16; 16] = [0; 16];
    for (channel, value) in main_fader_values.iter_mut().enumerate() {
        *value = SIM_FADER_POS[channel].load(Ordering::Relaxed);
        MAX_VALUES_FADER[channel].store(*value, Ordering::Relaxed);
    }

    let mut global_settings_fader_values: [u16; 16] =
        core::array::from_fn(|channel| get_fader_value_from_config(channel, &global_config));

    let mut fader_latches: [AnalogLatch; 16] = core::array::from_fn(|channel| {
        AnalogLatch::new(main_fader_values[channel], global_config.takeover_mode)
    });

    loop {
        // ~60Hz over all channels, like the hardware's 1ms-per-channel sweep
        Timer::after_millis(16).await;

        // global config mode: Alt, normal mode: Main
        let active_layer = if is_scene_button_pressed() {
            LatchLayer::Alt
        } else {
            LatchLayer::Main
        };

        for channel in 0..16 {
            let val = SIM_FADER_POS[channel].load(Ordering::Relaxed);
            let latch = &mut fader_latches[channel];

            let target_value = match active_layer {
                LatchLayer::Main => main_fader_values[channel],
                LatchLayer::Alt => global_settings_fader_values[channel],
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(val, active_layer, target_value) {
                let diff = (new_value as i32 - target_value as i32).abs();
                match active_layer {
                    LatchLayer::Main => {
                        if diff >= 4 {
                            event_publisher
                                .publish(InputEvent::FaderChange(channel))
                                .await;
                            main_fader_values[channel] = new_value;
                        }
                        MAX_VALUES_FADER[channel].store(new_value, Ordering::Relaxed)
                    }
                    LatchLayer::Alt => {
                        if diff >= 4 {
                            set_global_config_via_chan(channel, new_value);
                            global_settings_fader_values[channel] = new_value;
                        }
                    }
                    LatchLayer::Third => {}
                }
            }
        }
    }
}
