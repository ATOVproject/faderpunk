use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use libfp::GLOBAL_CHANNELS;
use serde::{Deserialize, Serialize};

use crate::{
    app::{GateJack, InJack, OutJack},
    storage,
};

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug)]
pub struct RuntimeState {
    pub clock_is_running: bool,
    pub out_jacks: [Option<OutJack>; GLOBAL_CHANNELS],
    pub in_jacks: [Option<InJack>; GLOBAL_CHANNELS],
    pub gate_jacks: [Option<GateJack>; GLOBAL_CHANNELS],
}

static STATE: Mutex<CriticalSectionRawMutex, RuntimeState> = Mutex::new(RuntimeState {
    clock_is_running: true,
    out_jacks: [None; GLOBAL_CHANNELS],
    in_jacks: [None; GLOBAL_CHANNELS],
    gate_jacks: [None; GLOBAL_CHANNELS],
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

// Gets configured set of each CV out app jack
pub async fn get_out_jacks() -> [Option<OutJack>; GLOBAL_CHANNELS] {
    STATE.lock().await.out_jacks
}

/// Gets configured set of each input app jack
pub async fn get_in_jacks() -> [Option<InJack>; GLOBAL_CHANNELS] {
    STATE.lock().await.in_jacks
}

/// Gets configured set of each gate out app jack
pub async fn get_gate_jacks() -> [Option<GateJack>; GLOBAL_CHANNELS] {
    STATE.lock().await.gate_jacks
}
