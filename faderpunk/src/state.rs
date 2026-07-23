use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use minicbor::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::storage;

/// Persisted to FRAM as CBOR. Adding/removing fields is migration-free as long
/// as every field carries `#[cbor(default)]` and a fresh `#[n(N)]`. See the
/// `GlobalConfig` doc comment in `libfp::lib` for the full convention.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Copy, Debug)]
pub struct RuntimeState {
    #[n(0)]
    #[cbor(default)]
    pub clock_is_running: bool,
}

/// A factory-fresh unit (empty/unreadable FRAM) boots with the internal
/// clock running, rather than requiring the user to press Start first.
impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            clock_is_running: true,
        }
    }
}

static STATE: Mutex<CriticalSectionRawMutex, RuntimeState> = Mutex::new(RuntimeState {
    clock_is_running: true,
});

pub async fn init_state() {
    let loaded_state = storage::load_runtime_state().await;
    let mut state = STATE.lock().await;
    *state = loaded_state;
}

/// Modifies the runtime state using a closure and persists it to storage if changed.
/// The closure should return `true` if the state was changed.
pub async fn update_state<F>(modifier: F)
where
    F: FnOnce(&mut RuntimeState) -> bool,
{
    let state_to_store: Option<RuntimeState>;

    {
        let mut state = STATE.lock().await;
        let changed = modifier(&mut state);
        if changed {
            state_to_store = Some(*state);
        } else {
            state_to_store = None;
        }
    }

    // If the state was changed, write it to persistent storage
    if let Some(state_val) = state_to_store {
        storage::store_runtime_state(&state_val).await;
    }
}

pub async fn is_clock_running() -> bool {
    STATE.lock().await.clock_is_running
}
