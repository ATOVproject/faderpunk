use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    mutex::Mutex,
    signal::Signal,
    watch::Watch,
};
use embassy_time::Timer;
use static_cell::StaticCell;

use libfp::{Layout, GLOBAL_CHANNELS};

use crate::apps::spawn_app_by_id;

// Receiver: layout spawn loop
const LAYOUT_WATCH_SUBSCRIBERS: usize = 1;

pub static LAYOUT_WATCH: Watch<CriticalSectionRawMutex, Layout, LAYOUT_WATCH_SUBSCRIBERS> =
    Watch::new_with(Layout::new());

pub static LAYOUT_MANAGER: StaticCell<LayoutManager> = StaticCell::new();

pub struct LayoutManager {
    exit_signals: [Signal<NoopRawMutex, bool>; GLOBAL_CHANNELS],
    layout: Mutex<NoopRawMutex, [Option<(u8, usize)>; GLOBAL_CHANNELS]>,
    spawner: Spawner,
}

impl LayoutManager {
    pub fn new(spawner: Spawner) -> Self {
        Self {
            exit_signals: [const { Signal::new() }; GLOBAL_CHANNELS],
            layout: Mutex::new([None; GLOBAL_CHANNELS]),
            spawner,
        }
    }

    async fn exit_app(&self, start_channel: usize) {
        let mut layout = self.layout.lock().await;
        if layout[start_channel].is_some() {
            layout[start_channel] = None;
            drop(layout);

            self.exit_signals[start_channel].signal(true);
            Timer::after_millis(10).await;
        }
    }

    pub async fn spawn_layout(&'static self, layout: Layout) {
        // Build a representation of the desired layout, mapping start_channel to (app_id, channels)
        let mut desired_layout: [Option<(u8, usize)>; GLOBAL_CHANNELS] = [None; GLOBAL_CHANNELS];
        for (app_id, start_channel, channels) in layout.iter() {
            if start_channel < GLOBAL_CHANNELS {
                desired_layout[start_channel] = Some((app_id, channels));
            }
        }

        // Pass 1: Exit apps that are no longer desired or are different
        for start_channel in 0..GLOBAL_CHANNELS {
            let current_app = {
                let current_layout = self.layout.lock().await;
                current_layout[start_channel]
            };

            // If an app is running but the desired layout is different (or empty), exit the app
            if current_app.is_some() && current_app != desired_layout[start_channel] {
                self.exit_app(start_channel).await;
            }
        }

        // Pass 2: Spawn new or changed apps
        for start_channel in 0..GLOBAL_CHANNELS {
            if let Some((app_id, channels)) = desired_layout[start_channel] {
                let should_spawn = {
                    let current_layout = self.layout.lock().await;
                    current_layout[start_channel].is_none()
                };

                if should_spawn {
                    spawn_app_by_id(app_id, start_channel, self.spawner, &self.exit_signals);
                    let mut current_layout = self.layout.lock().await;
                    current_layout[start_channel] = Some((app_id, channels));
                }
            }
        }
    }
}
