use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::Timer;
use static_cell::StaticCell;

use libfp::{Layout, GLOBAL_CHANNELS};

use crate::apps::spawn_app_by_id;

pub static LAYOUT_MANAGER: StaticCell<LayoutManager> = StaticCell::new();

pub struct LayoutManager {
    exit_signals: [Signal<NoopRawMutex, bool>; GLOBAL_CHANNELS],
    layout: Mutex<NoopRawMutex, [Option<u8>; GLOBAL_CHANNELS]>,
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
        self.exit_signals[start_channel].signal(true);
        Timer::after_millis(10).await;
    }

    pub async fn spawn_layout(&'static self, layout: Layout) {
        for (app_id, start_channel, channels) in layout.iter() {
            let current_app = {
                let own_layout = self.layout.lock().await;
                own_layout[start_channel]
            };

            // Only spawn if the app isn't already the one running.
            if current_app != Some(app_id) {
                for channel in start_channel..(start_channel + channels) {
                    let should_exit = {
                        let own_layout = self.layout.lock().await;
                        own_layout[channel].is_some()
                    };
                    if should_exit {
                        self.exit_app(channel).await;
                        let mut layout = self.layout.lock().await;
                        layout[channel] = None;
                    }
                }
                // Spawn the app!
                spawn_app_by_id(app_id, start_channel, self.spawner, &self.exit_signals).await;
                let mut own_layout = self.layout.lock().await;
                own_layout[start_channel] = Some(app_id);
            }
        }

        // Exit any apps that are still running beyond the new layout
        if let Some(first_free) = layout.first_free() {
            for channel in first_free..GLOBAL_CHANNELS {
                let should_exit = {
                    let layout = self.layout.lock().await;
                    layout[channel].is_some()
                };
                if should_exit {
                    self.exit_app(channel).await;
                    let mut layout = self.layout.lock().await;
                    layout[channel] = None;
                }
            }
        }
    }
}
