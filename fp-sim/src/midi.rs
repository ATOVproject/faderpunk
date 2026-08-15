use std::sync::Arc;

#[cfg(unix)]
use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;

use fp_sim_protocol::HostToCore;

use crate::core_process::CoreSender;

#[cfg(unix)]
const CLIENT_NAME: &str = "Faderpunk Sim";
#[cfg(unix)]
const CONFIG_PORT_NAME: &str = "Faderpunk Sim Config";

#[cfg(windows)]
const CLIENT_NAME: &str = "Faderpunk Sim";
#[cfg(windows)]
const WIN_PERFORMANCE_IN: &str = "Faderpunk Sim In";
#[cfg(windows)]
const WIN_PERFORMANCE_OUT: &str = "Faderpunk Sim Out";
#[cfg(windows)]
const WIN_CONFIG_IN: &str = "Faderpunk Sim Config In";
#[cfg(windows)]
const WIN_CONFIG_OUT: &str = "Faderpunk Sim Config Out";

pub struct MidiOutputs {
    performance: Mutex<MidiOutputConnection>,
    config: Mutex<MidiOutputConnection>,
}

impl MidiOutputs {
    #[cfg(unix)]
    pub fn create() -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            performance: Mutex::new(create_virtual_out(CLIENT_NAME)?),
            config: Mutex::new(create_virtual_out(CONFIG_PORT_NAME)?),
        }))
    }

    #[cfg(windows)]
    pub fn create() -> Result<Arc<Self>, String> {
        let perf_out = MidiOutput::new(CLIENT_NAME).map_err(|error| error.to_string())?;
        let perf_port = find_output_port(&perf_out, WIN_PERFORMANCE_OUT)?;
        let perf_conn = perf_out
            .connect(&perf_port, WIN_PERFORMANCE_OUT)
            .map_err(|error| error.to_string())?;

        let cfg_out = MidiOutput::new(CLIENT_NAME).map_err(|error| error.to_string())?;
        let cfg_port = find_output_port(&cfg_out, WIN_CONFIG_OUT)?;
        let cfg_conn = cfg_out
            .connect(&cfg_port, WIN_CONFIG_OUT)
            .map_err(|error| error.to_string())?;

        Ok(Arc::new(Self {
            performance: Mutex::new(perf_conn),
            config: Mutex::new(cfg_conn),
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
    #[cfg(unix)]
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

    #[cfg(windows)]
    pub fn open(sender: CoreSender, outputs: Arc<MidiOutputs>) -> Result<Self, String> {
        let mut perf_in = MidiInput::new(CLIENT_NAME).map_err(|error| error.to_string())?;
        perf_in.ignore(Ignore::None);
        let perf_port = find_input_port(&perf_in, WIN_PERFORMANCE_IN)?;
        let performance_sender = sender.clone();
        let perf_conn = perf_in
            .connect(
                &perf_port,
                WIN_PERFORMANCE_IN,
                move |_timestamp, bytes, _| {
                    performance_sender.send(HostToCore::PerformanceMidi(bytes.to_vec()));
                },
                (),
            )
            .map_err(|error| error.to_string())?;

        let mut cfg_in = MidiInput::new(CLIENT_NAME).map_err(|error| error.to_string())?;
        cfg_in.ignore(Ignore::None);
        let cfg_port = find_input_port(&cfg_in, WIN_CONFIG_IN)?;
        let cfg_conn = cfg_in
            .connect(
                &cfg_port,
                WIN_CONFIG_IN,
                move |_timestamp, bytes, _| {
                    sender.send(HostToCore::ConfigMidi(bytes.to_vec()));
                },
                (),
            )
            .map_err(|error| error.to_string())?;

        log::info!(
            "Windows MIDI ports connected: \"{WIN_PERFORMANCE_IN}\"/\"{WIN_PERFORMANCE_OUT}\" (performance), \"{WIN_CONFIG_IN}\"/\"{WIN_CONFIG_OUT}\" (config)"
        );
        Ok(Self {
            _performance_input: perf_conn,
            _config_input: cfg_conn,
            _outputs: outputs,
        })
    }
}

#[cfg(unix)]
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

#[cfg(unix)]
fn create_virtual_out(port_name: &str) -> Result<MidiOutputConnection, String> {
    MidiOutput::new(CLIENT_NAME)
        .map_err(|error| error.to_string())?
        .create_virtual(port_name)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn find_input_port(input: &MidiInput, target_name: &str) -> Result<midir::MidiInputPort, String> {
    let ports = input.ports();
    let mut available = Vec::new();
    for port in &ports {
        match input.port_name(port) {
            Ok(name) => {
                if name == target_name {
                    return Ok(port.clone());
                }
                available.push(name);
            }
            Err(_) => continue,
        }
    }
    let available_str = if available.is_empty() {
        "<none>".to_string()
    } else {
        available
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(format!(
        "Windows MIDI input port \"{target_name}\" not found. Install and run loopMIDI, then create exactly: \"Faderpunk Sim In\", \"Faderpunk Sim Out\", \"Faderpunk Sim Config In\", \"Faderpunk Sim Config Out\". Available input ports: {available_str}"
    ))
}

#[cfg(windows)]
fn find_output_port(
    output: &MidiOutput,
    target_name: &str,
) -> Result<midir::MidiOutputPort, String> {
    let ports = output.ports();
    let mut available = Vec::new();
    for port in &ports {
        match output.port_name(port) {
            Ok(name) => {
                if name == target_name {
                    return Ok(port.clone());
                }
                available.push(name);
            }
            Err(_) => continue,
        }
    }
    let available_str = if available.is_empty() {
        "<none>".to_string()
    } else {
        available
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(format!(
        "Windows MIDI output port \"{target_name}\" not found. Install and run loopMIDI, then create exactly: \"Faderpunk Sim In\", \"Faderpunk Sim Out\", \"Faderpunk Sim Config In\", \"Faderpunk Sim Config Out\". Available output ports: {available_str}"
    ))
}
