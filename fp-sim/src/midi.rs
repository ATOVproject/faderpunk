use std::sync::Arc;

use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;

use fp_sim_protocol::HostToCore;

use crate::core_process::CoreSender;

const CLIENT_NAME: &str = "Faderpunk Sim";
const CONFIG_PORT_NAME: &str = "Faderpunk Sim Config";

pub struct MidiOutputs {
    performance: Mutex<MidiOutputConnection>,
    config: Mutex<MidiOutputConnection>,
}

impl MidiOutputs {
    pub fn create() -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            performance: Mutex::new(create_virtual_out(CLIENT_NAME)?),
            config: Mutex::new(create_virtual_out(CONFIG_PORT_NAME)?),
        }))
    }

    pub fn send_performance(&self, bytes: &[u8]) {
        if let Err(error) = self.performance.lock().send(bytes) {
            log::warn!("Failed to send performance MIDI: {error}");
        }
    }

    pub fn send_config(&self, bytes: &[u8]) {
        if let Err(error) = self.config.lock().send(bytes) {
            log::warn!("Failed to send config MIDI: {error}");
        }
    }
}

pub struct MidiPorts {
    _performance_input: MidiInputConnection<()>,
    _config_input: MidiInputConnection<()>,
    _outputs: Arc<MidiOutputs>,
}

impl MidiPorts {
    pub fn open(sender: CoreSender, outputs: Arc<MidiOutputs>) -> Result<Self, String> {
        let performance_sender = sender.clone();
        let performance_input = create_virtual_in(CLIENT_NAME, move |bytes| {
            performance_sender.send(HostToCore::PerformanceMidi(bytes.to_vec()));
        })?;
        let config_input = create_virtual_in(CONFIG_PORT_NAME, move |bytes| {
            sender.send(HostToCore::ConfigMidi(bytes.to_vec()));
        })?;
        log::info!("Virtual MIDI ports: \"{CLIENT_NAME}\" (performance), \"{CONFIG_PORT_NAME}\"");
        Ok(Self {
            _performance_input: performance_input,
            _config_input: config_input,
            _outputs: outputs,
        })
    }
}

fn create_virtual_in(
    port_name: &str,
    mut receive: impl FnMut(&[u8]) + Send + 'static,
) -> Result<MidiInputConnection<()>, String> {
    let mut input = MidiInput::new(CLIENT_NAME).map_err(|error| error.to_string())?;
    input.ignore(Ignore::None);
    input
        .create_virtual(port_name, move |_timestamp, bytes, _| receive(bytes), ())
        .map_err(|error| error.to_string())
}

fn create_virtual_out(port_name: &str) -> Result<MidiOutputConnection, String> {
    MidiOutput::new(CLIENT_NAME)
        .map_err(|error| error.to_string())?
        .create_virtual(port_name)
        .map_err(|error| error.to_string())
}
