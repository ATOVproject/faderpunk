mod core_process;
mod midi;
mod state;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

use core_process::CoreManager;
use midi::{MidiOutputs, MidiPorts};
use state::PanelState;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = match Options::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let state = Arc::new(PanelState::load());
    let outputs = MidiOutputs::create().unwrap_or_else(|error| {
        eprintln!("Failed to create virtual MIDI outputs: {error}");
        std::process::exit(1);
    });
    let manager = CoreManager::start(options.project, Arc::clone(&state), Arc::clone(&outputs));
    let sender = manager.sender();
    let ports = MidiPorts::open(sender.clone(), outputs).unwrap_or_else(|error| {
        eprintln!("Failed to create virtual MIDI inputs: {error}");
        std::process::exit(1);
    });

    if options.headless {
        log::info!("Headless parent running; Enter toggles transport, q+Enter quits");
        loop {
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(0) => std::thread::sleep(std::time::Duration::from_millis(200)),
                Ok(_) if line.trim().eq_ignore_ascii_case("q") => break,
                Ok(_) => {
                    sender.send(fp_sim_protocol::HostToCore::TransportToggle);
                }
                Err(error) => {
                    log::warn!("Headless input failed: {error}");
                    break;
                }
            }
        }
    } else {
        let native_options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_title("Faderpunk Simulator")
                .with_inner_size([1340.0, 760.0])
                .with_min_inner_size([980.0, 620.0]),
            ..Default::default()
        };
        let ui_state = Arc::clone(&state);
        let sender = sender.clone();
        if let Err(error) = eframe::run_native(
            "Faderpunk Simulator",
            native_options,
            Box::new(move |_creation_context| {
                Ok(Box::new(ui::FaderpunkPanel::new(ui_state, sender)))
            }),
        ) {
            log::error!("Panel UI failed: {error}");
        }
    }

    state.persist();
    manager.shutdown();
    drop(ports);
}

struct Options {
    project: Option<PathBuf>,
    headless: bool,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut project = None;
        let mut headless = std::env::var_os("FP_SIM_HEADLESS").is_some();
        let mut arguments = std::env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--project") => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| "--project requires a path".to_owned())?;
                    project = Some(PathBuf::from(value));
                }
                Some("--headless") => headless = true,
                Some("--help" | "-h") => {
                    println!(
                        "fp-sim [--project PATH] [--headless]\n\n\
                         --project PATH  external app crate (exactly one binary)\n\
                         --headless      run parent, MIDI, watcher, and child without the panel"
                    );
                    std::process::exit(0);
                }
                _ => return Err(format!("unknown argument: {}", argument.to_string_lossy())),
            }
        }
        Ok(Self { project, headless })
    }
}
