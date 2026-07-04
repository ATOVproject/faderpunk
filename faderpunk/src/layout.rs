use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    mutex::Mutex,
    signal::Signal,
    watch::Watch,
};
use embassy_time::Timer;
use static_cell::StaticCell;

use libfp::{InnerLayout, Layout, GLOBAL_CHANNELS};

use crate::apps::spawn_app_by_id;

// Receivers: layout spawn loop, configure
const LAYOUT_WATCH_SUBSCRIBERS: usize = 2;

pub static LAYOUT_WATCH: Watch<CriticalSectionRawMutex, Layout, LAYOUT_WATCH_SUBSCRIBERS> =
    Watch::new();

/// Signal to force respawn all apps
pub static FORCE_RESPAWN_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// A scoped, non-persisting request to evict or restore a single channel's
/// app, used by the V/Oct calibration wizard to temporarily free a jack an
/// app is using without touching the persisted layout.
pub enum EvictionCmd {
    /// Exit whatever app is running on this start_channel, if any.
    Evict(usize),
    /// Respawn (app_id, channels, layout_id) on this start_channel.
    Restore(usize, u8, usize, u8),
}

pub static LAYOUT_EVICTION_REQ: Signal<CriticalSectionRawMutex, EvictionCmd> = Signal::new();
pub static LAYOUT_EVICTION_RES: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub static LAYOUT_MANAGER: StaticCell<LayoutManager> = StaticCell::new();

pub struct LayoutManager {
    exit_signals: [Signal<NoopRawMutex, bool>; GLOBAL_CHANNELS],
    layout: Mutex<NoopRawMutex, InnerLayout>,
    /// Channels currently on loan for V/Oct calibration (see `EvictionCmd`).
    /// `spawn_layout`'s reconciliation pass must not spawn into a held
    /// channel even if the persisted layout wants an app there, since a
    /// held channel is mid-calibration and not actually free.
    held: Mutex<NoopRawMutex, [bool; GLOBAL_CHANNELS]>,
    spawner: Spawner,
}

impl LayoutManager {
    pub fn new(spawner: Spawner) -> Self {
        Self {
            exit_signals: [const { Signal::new() }; GLOBAL_CHANNELS],
            layout: Mutex::new([None; GLOBAL_CHANNELS]),
            held: Mutex::new([false; GLOBAL_CHANNELS]),
            spawner,
        }
    }

    /// Mark `start_channel` as held (or release it) for a temporary V/Oct
    /// calibration eviction, so ordinary layout reconciliation leaves it
    /// alone until it's released.
    pub(crate) async fn set_held(&self, start_channel: usize, held: bool) {
        self.held.lock().await[start_channel] = held;
    }

    pub(crate) async fn exit_app(&self, start_channel: usize) {
        let mut layout = self.layout.lock().await;
        if layout[start_channel].is_some() {
            layout[start_channel] = None;
            drop(layout);

            self.exit_signals[start_channel].signal(true);
            Timer::after_millis(10).await;
        }
    }

    /// Force respawn all apps by exiting them all and then respawning with the given layout
    pub async fn respawn_all(&'static self, layout: &Layout) {
        // Exit all currently running apps
        for start_channel in 0..GLOBAL_CHANNELS {
            self.exit_app(start_channel).await;
        }

        // Now spawn the desired layout
        self.spawn_layout(layout).await;
    }

    /// Spawn a single (app_id, channels, layout_id) onto `start_channel` if
    /// nothing is currently running there. Used to restore an app that was
    /// temporarily evicted (e.g. for V/Oct calibration) without touching the
    /// persisted layout.
    pub(crate) async fn spawn_one(
        &'static self,
        start_channel: usize,
        app_id: u8,
        channels: usize,
        layout_id: u8,
    ) {
        let mut current_layout = self.layout.lock().await;
        if current_layout[start_channel].is_none() {
            spawn_app_by_id(
                app_id,
                start_channel,
                layout_id,
                self.spawner,
                &self.exit_signals,
            );
            current_layout[start_channel] = Some((app_id, channels, layout_id));
        }
    }

    pub async fn spawn_layout(&'static self, layout: &Layout) -> bool {
        let mut changed = false;

        // Build a representation of the desired layout, mapping start_channel to (app_id, channels)
        let mut desired_layout: InnerLayout = [None; GLOBAL_CHANNELS];
        for (app_id, start_channel, channels, layout_id) in layout.iter() {
            if start_channel < GLOBAL_CHANNELS {
                desired_layout[start_channel] = Some((app_id, channels, layout_id));
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
                changed = true;
            }
        }

        // Pass 2: Spawn new or changed apps
        for start_channel in 0..GLOBAL_CHANNELS {
            if let Some((app_id, channels, layout_id)) = desired_layout[start_channel] {
                let should_spawn = {
                    let current_layout = self.layout.lock().await;
                    current_layout[start_channel].is_none()
                };
                let held = self.held.lock().await[start_channel];

                if should_spawn && !held {
                    spawn_app_by_id(
                        app_id,
                        start_channel,
                        layout_id,
                        self.spawner,
                        &self.exit_signals,
                    );
                    let mut current_layout = self.layout.lock().await;
                    current_layout[start_channel] = Some((app_id, channels, layout_id));
                    changed = true;
                }
            }
        }

        changed
    }
}
