use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use libfp::GLOBAL_CHANNELS;
use minicbor::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{
    app::{GateJack, InJack, OutJack},
    storage,
};

/// Persisted to FRAM as CBOR. Adding/removing fields is migration-free as long
/// as every field carries `#[cbor(default)]` and a fresh `#[n(N)]`. See the
/// `GlobalConfig` doc comment in `libfp::lib` for the full convention.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Copy, Default, Debug)]
pub struct RuntimeState {
    #[n(0)]
    #[cbor(default)]
    pub clock_is_running: bool,
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

/// Runtime-only registry of which jack is currently configured on each global
/// channel. Rebuilt every time apps spawn (each app re-registers its jacks on
/// init) — never persisted to FRAM, unlike `RuntimeState`.
struct JackRegistry {
    out_jacks: [Option<OutJack>; GLOBAL_CHANNELS],
    in_jacks: [Option<InJack>; GLOBAL_CHANNELS],
    gate_jacks: [Option<GateJack>; GLOBAL_CHANNELS],
}

static JACKS: Mutex<CriticalSectionRawMutex, JackRegistry> = Mutex::new(JackRegistry {
    out_jacks: [None; GLOBAL_CHANNELS],
    in_jacks: [None; GLOBAL_CHANNELS],
    gate_jacks: [None; GLOBAL_CHANNELS],
});

pub async fn register_out_jack(global_chan: usize, jack: OutJack) {
    JACKS.lock().await.out_jacks[global_chan] = Some(jack);
}

pub async fn register_in_jack(global_chan: usize, jack: InJack) {
    JACKS.lock().await.in_jacks[global_chan] = Some(jack);
}

pub async fn register_gate_jack(global_chan: usize, jack: GateJack) {
    JACKS.lock().await.gate_jacks[global_chan] = Some(jack);
}

/// Clears any registered jack for the given global channel. Called when an
/// app exits so a despawned app's jack doesn't outlive it in the registry.
pub async fn clear_jacks(global_chan: usize) {
    let mut jacks = JACKS.lock().await;
    jacks.out_jacks[global_chan] = None;
    jacks.in_jacks[global_chan] = None;
    jacks.gate_jacks[global_chan] = None;
}

/// Gets configured set of each CV out app jack
pub async fn get_out_jacks() -> [Option<OutJack>; GLOBAL_CHANNELS] {
    JACKS.lock().await.out_jacks
}

/// Gets configured set of each input app jack
pub async fn get_in_jacks() -> [Option<InJack>; GLOBAL_CHANNELS] {
    JACKS.lock().await.in_jacks
}

/// Gets configured set of each gate out app jack
pub async fn get_gate_jacks() -> [Option<GateJack>; GLOBAL_CHANNELS] {
    JACKS.lock().await.gate_jacks
}
