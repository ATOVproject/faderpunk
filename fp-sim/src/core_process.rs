use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use fp_sim_protocol::{CoreToHost, HostToCore};

use crate::midi::MidiOutputs;
use crate::state::PanelState;

#[derive(Clone)]
pub struct CoreSender {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl CoreSender {
    fn new() -> Self {
        Self {
            stdin: Arc::new(Mutex::new(None)),
        }
    }

    pub fn send(&self, message: HostToCore) -> bool {
        let mut guard = self.stdin.lock();
        let Some(stdin) = guard.as_mut() else {
            return false;
        };
        if fp_sim_protocol::write_frame(stdin, &message).is_err() {
            *guard = None;
            return false;
        }
        true
    }

    fn replace(&self, stdin: ChildStdin) {
        *self.stdin.lock() = Some(stdin);
    }

    fn disconnect(&self) {
        *self.stdin.lock() = None;
    }
}
pub struct CoreManager {
    sender: CoreSender,
    control: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CoreManager {
    pub fn start(project: Option<PathBuf>, state: Arc<PanelState>, midi: Arc<MidiOutputs>) -> Self {
        let sender = CoreSender::new();
        let (control, receiver) = mpsc::channel();
        let thread_sender = sender.clone();
        let thread = std::thread::Builder::new()
            .name("fp-sim-core-manager".into())
            .spawn(move || run_manager(project, state, midi, thread_sender, receiver))
            .expect("failed to start simulator core manager");
        Self {
            sender,
            control,
            thread: Some(thread),
        }
    }

    pub fn sender(&self) -> CoreSender {
        self.sender.clone()
    }

    pub fn shutdown(mut self) {
        let _ = self.control.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct BuildSpec {
    manifest: PathBuf,
    package: Option<&'static str>,
    working_dir: PathBuf,
    watch_paths: Vec<(PathBuf, RecursiveMode)>,
}

impl BuildSpec {
    fn new(project: Option<PathBuf>) -> Result<Self, String> {
        let working_dir = std::env::current_dir().map_err(|error| error.to_string())?;
        if let Some(project) = project {
            let manifest = if project.is_absolute() {
                if project.is_dir() {
                    project.join("Cargo.toml")
                } else {
                    project
                }
            } else {
                let resolved = working_dir.join(project);
                if resolved.is_dir() {
                    resolved.join("Cargo.toml")
                } else {
                    resolved
                }
            };
            if !manifest.is_file() {
                return Err(format!(
                    "app project manifest not found: {}",
                    manifest.display()
                ));
            }
            let project_dir = manifest
                .parent()
                .expect("manifest path must have a parent directory")
                .to_path_buf();
            Ok(Self {
                manifest: manifest.clone(),
                package: None,
                working_dir,
                watch_paths: vec![
                    (project_dir.join("src"), RecursiveMode::Recursive),
                    (manifest, RecursiveMode::NonRecursive),
                ],
            })
        } else {
            let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("fp-sim must live in the workspace")
                .to_path_buf();
            let manifest = repository.join("Cargo.toml");
            Ok(Self {
                manifest,
                package: Some("fp-sim-core"),
                working_dir,
                watch_paths: vec![
                    (repository.join("fp-sim-core/src"), RecursiveMode::Recursive),
                    (repository.join("fp-core/src"), RecursiveMode::Recursive),
                    (repository.join("libfp/src"), RecursiveMode::Recursive),
                ],
            })
        }
    }

    fn cargo_command(&self, cargo: &OsStr, frozen: bool) -> Command {
        let manifest_dir = self
            .manifest
            .parent()
            .expect("manifest path must have a parent directory");
        let mut command = Command::new(cargo);
        command
            .current_dir(manifest_dir)
            .arg("build")
            .arg("--manifest-path")
            .arg(&self.manifest)
            .arg("--message-format=json-render-diagnostics");
        if let Some(package) = self.package {
            command.arg("--package").arg(package);
        }
        if frozen {
            command.arg("--frozen");
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command
    }

    fn build(&self, state: &PanelState) -> Result<PathBuf, String> {
        state.set_status("Building simulator core…");
        let cargo = std::env::var_os("FP_SIM_CARGO").unwrap_or_else(|| "cargo".into());
        let frozen = std::env::var_os("FP_SIM_CARGO_FROZEN").is_some();
        let mut command = self.cargo_command(&cargo, frozen);
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to start cargo: {error}"))?;
        let stdout = child.stdout.take().expect("cargo stdout was piped");
        let stderr = child.stderr.take().expect("cargo stderr was piped");
        let stderr_thread = std::thread::Builder::new()
            .name("fp-sim-cargo-stderr".into())
            .spawn(move || {
                let mut summary = None;
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    eprintln!("{line}");
                    if summary.is_none() && line.trim_start().starts_with("error") {
                        summary = Some(line.trim().to_owned());
                    }
                }
                summary
            })
            .map_err(|error| error.to_string())?;
        let mut executables = Vec::new();
        let mut error_summary = None;
        for line in BufReader::new(stdout).lines() {
            let line = line.map_err(|error| error.to_string())?;
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if message["reason"] == "compiler-message" {
                if let Some(rendered) = message["message"]["rendered"].as_str() {
                    eprint!("{rendered}");
                }
                if error_summary.is_none() && message["message"]["level"] == "error" {
                    error_summary = message["message"]["message"]
                        .as_str()
                        .map(ToOwned::to_owned);
                }
                continue;
            }
            if message["reason"] != "compiler-artifact" {
                continue;
            }
            let is_binary = message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"));
            if is_binary {
                if let Some(executable) = message["executable"].as_str() {
                    executables.push(PathBuf::from(executable));
                }
            }
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        let stderr_summary = stderr_thread
            .join()
            .map_err(|_| "Cargo stderr reader panicked".to_owned())?;
        if error_summary.is_none() {
            error_summary = stderr_summary;
        }
        if !status.success() {
            return Err(match error_summary {
                Some(summary) => format!("Build failed: {summary}"),
                None => format!("cargo build failed with {status}"),
            });
        }
        executables.sort();
        executables.dedup();
        match executables.as_slice() {
            [executable] => Ok(executable.clone()),
            [] => Err("cargo produced no simulator child binary".into()),
            _ => Err("app project must produce exactly one binary".into()),
        }
    }
}

fn run_manager(
    project: Option<PathBuf>,
    state: Arc<PanelState>,
    midi: Arc<MidiOutputs>,
    sender: CoreSender,
    control_rx: mpsc::Receiver<()>,
) {
    let spec = match BuildSpec::new(project) {
        Ok(spec) => spec,
        Err(error) => {
            state.set_status(error);
            return;
        }
    };
    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = create_watcher(watch_tx).ok();
    if let Some(watcher) = watcher.as_mut() {
        for (path, mode) in &spec.watch_paths {
            if let Err(error) = watcher.watch(path, *mode) {
                log::warn!("Cannot watch {}: {error}", path.display());
            }
        }
    }

    let mut child: Option<Child> = None;
    let mut executable: Option<PathBuf> = None;
    let mut rebuild = true;
    let mut last_change = Instant::now();

    loop {
        if control_rx.try_recv().is_ok() {
            stop_child(&sender, child.as_mut());
            return;
        }
        while watch_rx.try_recv().is_ok() {
            rebuild = true;
            last_change = Instant::now();
        }

        if rebuild && last_change.elapsed() >= Duration::from_millis(250) {
            match spec.build(&state) {
                Ok(built) => {
                    executable = Some(built);
                    stop_child(&sender, child.as_mut());
                    child = None;
                    match spawn_child(executable.as_ref().unwrap(), &spec, &state, &midi, &sender) {
                        Ok(spawned) => child = Some(spawned),
                        Err(error) => state.set_status(error),
                    }
                }
                Err(error) => {
                    state.set_status(error.clone());
                    log::error!("{error}");
                }
            }
            rebuild = false;
        }

        let child_exit = child.as_mut().and_then(|running| match running.try_wait() {
            Ok(status) => status,
            Err(error) => {
                log::error!("Failed to inspect simulator core process: {error}");
                None
            }
        });
        if let Some(status) = child_exit {
            sender.disconnect();
            state
                .core_ready
                .store(false, std::sync::atomic::Ordering::Relaxed);
            state.set_status(format!("Simulator core exited ({status}); restarting…"));
            log::warn!("Simulator core exited with {status}; restarting");
            child = None;
            std::thread::sleep(Duration::from_millis(250));
            if let Some(path) = executable.as_ref() {
                match spawn_child(path, &spec, &state, &midi, &sender) {
                    Ok(spawned) => child = Some(spawned),
                    Err(error) => state.set_status(error),
                }
            }
        }

        match control_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => {
                stop_child(&sender, child.as_mut());
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop_child(&sender, child.as_mut());
                return;
            }
        }
    }
}

fn create_watcher(sender: mpsc::Sender<()>) -> notify::Result<RecommendedWatcher> {
    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return;
        }
        let relevant = event.paths.iter().any(|path| {
            path.extension()
                .is_some_and(|extension| extension == "rs" || extension == "toml")
        });
        if relevant {
            let _ = sender.send(());
        }
    })
}

fn spawn_child(
    executable: &Path,
    spec: &BuildSpec,
    state: &Arc<PanelState>,
    midi: &Arc<MidiOutputs>,
    sender: &CoreSender,
) -> Result<Child, String> {
    state
        .core_ready
        .store(false, std::sync::atomic::Ordering::Relaxed);
    state.set_status("Starting simulator core…");
    let mut child = Command::new(executable)
        .current_dir(&spec.working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", executable.display()))?;
    sender.replace(child.stdin.take().expect("child stdin was piped"));
    let stdout = child.stdout.take().expect("child stdout was piped");
    let state = Arc::clone(state);
    let midi = Arc::clone(midi);
    let sender = sender.clone();
    std::thread::Builder::new()
        .name("fp-sim-core-output".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match fp_sim_protocol::read_frame::<CoreToHost>(&mut reader) {
                    Ok(Some(CoreToHost::Ready { firmware_version })) => {
                        for (target, value) in state.firmware_version.iter().zip([
                            firmware_version.0,
                            firmware_version.1,
                            firmware_version.2,
                        ]) {
                            target.store(value, std::sync::atomic::Ordering::Relaxed);
                        }
                        state
                            .core_ready
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        state.set_status("Core running — watching app sources");
                        log::info!(
                            "Core ready v{}.{}.{}; replaying physical panel state",
                            firmware_version.0,
                            firmware_version.1,
                            firmware_version.2
                        );
                        for input in state.input_snapshot() {
                            sender.send(input);
                        }
                    }
                    Ok(Some(CoreToHost::Snapshot(snapshot))) => state.apply_snapshot(snapshot),
                    Ok(Some(CoreToHost::PerformanceMidi(bytes))) => midi.send_performance(&bytes),
                    Ok(Some(CoreToHost::ConfigMidi(bytes))) => midi.send_config(&bytes),
                    Ok(None) => break,
                    Err(error) => {
                        log::error!("Core IPC output failed: {error}");
                        break;
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(child)
}

fn stop_child(sender: &CoreSender, child: Option<&mut Child>) {
    let _ = sender.send(HostToCore::Shutdown);
    sender.disconnect();
    if let Some(child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_build_spec_command() {
        let spec = BuildSpec::new(None).expect("repository build spec should construct");
        assert_eq!(spec.package, Some("fp-sim-core"));
        assert!(spec.manifest.is_absolute());
        assert_eq!(spec.manifest.file_name(), Some(OsStr::new("Cargo.toml")));

        let cmd = spec.cargo_command(OsStr::new("cargo"), false);
        assert_eq!(cmd.get_program(), "cargo");
        assert_eq!(cmd.get_current_dir(), Some(spec.manifest.parent().unwrap()));

        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "build",
                "--manifest-path",
                spec.manifest.to_str().unwrap(),
                "--message-format=json-render-diagnostics",
                "--package",
                "fp-sim-core",
            ]
        );

        let frozen_cmd = spec.cargo_command(OsStr::new("custom-cargo"), true);
        assert_eq!(frozen_cmd.get_program(), "custom-cargo");
        let frozen_args: Vec<String> = frozen_cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            frozen_args,
            vec![
                "build",
                "--manifest-path",
                spec.manifest.to_str().unwrap(),
                "--message-format=json-render-diagnostics",
                "--package",
                "fp-sim-core",
                "--frozen",
            ]
        );
    }

    #[test]
    fn external_project_build_spec_command() {
        let temp_dir = std::env::temp_dir().join(format!(
            "fp_sim_test_ext_proj_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let manifest_path = temp_dir.join("Cargo.toml");
        std::fs::write(&manifest_path, "[package]\nname = \"dummy\"\n").expect("write manifest");

        let spec =
            BuildSpec::new(Some(temp_dir.clone())).expect("external build spec should construct");
        assert_eq!(spec.package, None);
        assert_eq!(spec.manifest, manifest_path);
        assert!(spec.manifest.is_absolute());

        let cmd = spec.cargo_command(OsStr::new("cargo"), false);
        assert_eq!(cmd.get_program(), "cargo");
        assert_eq!(cmd.get_current_dir(), Some(temp_dir.as_path()));

        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "build",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
                "--message-format=json-render-diagnostics",
            ]
        );

        let frozen_cmd = spec.cargo_command(OsStr::new("cargo"), true);
        let frozen_args: Vec<String> = frozen_cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            frozen_args,
            vec![
                "build",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
                "--message-format=json-render-diagnostics",
                "--frozen",
            ]
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
