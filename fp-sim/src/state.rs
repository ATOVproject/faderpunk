use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU16, AtomicU32, AtomicU8, Ordering};

use fp_sim_protocol::{CoreSnapshot, HostToCore, BUTTONS, CHANNELS, LEDS, PORTS};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct PersistedPanelState {
    faders: [u16; CHANNELS],
}

pub struct PanelState {
    pub faders: [AtomicU16; CHANNELS],
    pub buttons: [AtomicBool; BUTTONS],
    pub leds: [AtomicU32; LEDS],
    pub latched_faders: [AtomicU16; CHANNELS],
    pub adc: [AtomicU16; PORTS],
    pub dac: [AtomicU16; PORTS],
    pub port_modes: [AtomicU8; PORTS],
    pub port_ranges: [AtomicU8; PORTS],
    pub gates: [AtomicBool; PORTS],
    pub clock_running: AtomicBool,
    pub current_scene: AtomicU8,
    pub bpm_bits: AtomicU32,
    pub swing: AtomicI8,
    pub firmware_version: [AtomicU8; 3],
    pub core_ready: AtomicBool,
    status: Mutex<String>,
    persistence_path: PathBuf,
}

impl PanelState {
    pub fn load() -> Self {
        let persistence_path = std::env::var_os("FP_SIM_PANEL_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("fp-sim-panel.bin"));
        Self::load_from(persistence_path)
    }

    fn load_from(persistence_path: PathBuf) -> Self {
        let persisted = std::fs::read(&persistence_path)
            .ok()
            .and_then(|bytes| postcard::from_bytes::<PersistedPanelState>(&bytes).ok());
        let faders = persisted.map(|state| state.faders).unwrap_or([0; CHANNELS]);

        Self {
            faders: core::array::from_fn(|index| AtomicU16::new(faders[index])),
            buttons: [const { AtomicBool::new(false) }; BUTTONS],
            leds: [const { AtomicU32::new(0) }; LEDS],
            latched_faders: [const { AtomicU16::new(0) }; CHANNELS],
            adc: [const { AtomicU16::new(0) }; PORTS],
            dac: [const { AtomicU16::new(0) }; PORTS],
            port_modes: [const { AtomicU8::new(0) }; PORTS],
            port_ranges: [const { AtomicU8::new(0) }; PORTS],
            gates: [const { AtomicBool::new(false) }; PORTS],
            clock_running: AtomicBool::new(false),
            current_scene: AtomicU8::new(u8::MAX),
            bpm_bits: AtomicU32::new(120.0_f32.to_bits()),
            swing: AtomicI8::new(0),
            firmware_version: [const { AtomicU8::new(0) }; 3],
            core_ready: AtomicBool::new(false),
            status: Mutex::new("Starting simulator core…".into()),
            persistence_path,
        }
    }

    pub fn persist(&self) {
        let state = PersistedPanelState {
            faders: core::array::from_fn(|index| self.faders[index].load(Ordering::Relaxed)),
        };
        match postcard::to_stdvec(&state) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&self.persistence_path, bytes) {
                    log::warn!("Failed to persist panel state: {err}");
                }
            }
            Err(err) => log::warn!("Failed to encode panel state: {err}"),
        }
    }

    pub fn apply_snapshot(&self, snapshot: CoreSnapshot) {
        for (target, value) in self.leds.iter().zip(snapshot.leds) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.latched_faders.iter().zip(snapshot.latched_faders) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.adc.iter().zip(snapshot.adc) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.dac.iter().zip(snapshot.dac) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.port_modes.iter().zip(snapshot.port_modes) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.port_ranges.iter().zip(snapshot.port_ranges) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.gates.iter().zip(snapshot.gates) {
            target.store(value, Ordering::Relaxed);
        }
        self.clock_running
            .store(snapshot.clock_running, Ordering::Relaxed);
        self.current_scene
            .store(snapshot.current_scene.unwrap_or(u8::MAX), Ordering::Relaxed);
        self.bpm_bits
            .store(snapshot.bpm.to_bits(), Ordering::Relaxed);
        self.swing.store(snapshot.swing, Ordering::Relaxed);
    }

    pub fn input_snapshot(&self) -> Vec<HostToCore> {
        let mut messages = Vec::with_capacity(CHANNELS + BUTTONS + PORTS);
        messages.extend((0..CHANNELS).map(|channel| HostToCore::Fader {
            channel: channel as u8,
            value: self.faders[channel].load(Ordering::Relaxed),
        }));
        messages.extend((0..BUTTONS).map(|index| HostToCore::Button {
            index: index as u8,
            pressed: self.buttons[index].load(Ordering::Relaxed),
        }));
        messages.extend((0..PORTS).map(|port| HostToCore::Adc {
            port: port as u8,
            value: self.adc[port].load(Ordering::Relaxed),
        }));
        messages
    }

    pub fn set_status(&self, status: impl Into<String>) {
        *self.status.lock() = status.into();
    }

    pub fn status(&self) -> String {
        self.status.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_inputs_are_replayed_and_faders_persist() {
        let path = std::env::temp_dir().join(format!(
            "fp-sim-panel-state-test-{}.bin",
            std::process::id()
        ));
        let state = PanelState::load_from(path.clone());
        state.faders[3].store(2345, Ordering::Relaxed);
        state.buttons[17].store(true, Ordering::Relaxed);
        state.adc[19].store(987, Ordering::Relaxed);
        let replay = state.input_snapshot();
        assert!(replay.contains(&HostToCore::Button {
            index: 17,
            pressed: true,
        }));
        assert!(replay.contains(&HostToCore::Adc {
            port: 19,
            value: 987,
        }));
        state.persist();

        let restored = PanelState::load_from(path.clone());
        assert_eq!(restored.faders[3].load(Ordering::Relaxed), 2345);
        assert!(restored.input_snapshot().into_iter().any(|message| {
            message
                == HostToCore::Fader {
                    channel: 3,
                    value: 2345,
                }
        }));
        std::fs::remove_file(path).unwrap();
    }
}
