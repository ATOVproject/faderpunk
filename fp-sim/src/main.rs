//! Faderpunk desktop simulator (headless proof of concept).
//!
//! Runs the unmodified fp-core app/clock/config stack on the embassy std
//! executor, with virtual hardware behind the shared statics:
//! - MIDI: two virtual port pairs mirroring the hardware's USB cables —
//!   "Faderpunk Sim" (performance) and "Faderpunk Sim Config" (configurator)
//! - FRAM: a file-backed image (`fp-sim-fram.bin`, override with FP_SIM_FRAM)
//! - MAX11300/LEDs: logging stand-ins
//! - Transport: press Enter to start/stop the clock (the hardware's
//!   Shift+Scene), like on a fresh device the clock starts stopped
//!
//! Set FP_SIM_LFO=1 to force the LFO app onto channel 0 for testing.

mod hw;
mod midi;
mod storage;

use std::path::PathBuf;

use embassy_executor::{Executor, Spawner};
use embassy_futures::select::{select, Either};
use portable_atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;

use fp_core::layout::{LayoutManager, FORCE_RESPAWN_SIGNAL, LAYOUT_MANAGER, LAYOUT_WATCH};
use fp_core::storage::{load_global_config, load_layout, migrate_fram, store_layout};
use fp_core::tasks::clock::{
    metronome, run_clock_gatekeeper, run_unified_clock_engine, TransportCmd, TRANSPORT_CMD_CHANNEL,
};
use fp_core::tasks::global_config::GLOBAL_CONFIG_WATCH;
use fp_core::tasks::midi::midi_distributor;
use fp_core::{platform, state};

/// Firmware version reported to the configurator. Keep in sync with the
/// `faderpunk` crate version the simulator mirrors.
pub const FIRMWARE_VERSION: (u8, u8, u8) = (1, 11, 0);

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

static RNG_STATE: AtomicU32 = AtomicU32::new(0);

/// xorshift32 seeded from the clock — plenty for dice rolls and random apps.
fn rand_u16() -> u16 {
    let mut x = RNG_STATE.load(Ordering::Relaxed);
    while x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0x1234_5678)
            | 1;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    RNG_STATE.store(x, Ordering::Relaxed);
    (x >> 8) as u16
}

fn sys_reset() -> ! {
    log::info!("System reset requested — exiting (restart the simulator)");
    std::process::exit(0)
}

fn fram_path() -> PathBuf {
    std::env::var_os("FP_SIM_FRAM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fp-sim-fram.bin"))
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    platform::init(platform::Platform {
        rand_u16,
        sys_reset,
    });

    // Must exist before the executor starts; the input connections' callback
    // threads feed the RX channels for the whole process lifetime.
    let ports = midi::create_virtual_ports();

    spawn_stdin_transport_control();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(boot(spawner, ports)).unwrap();
    });
}

/// Headless stand-in for the hardware's Shift+Scene transport toggle:
/// pressing Enter starts/stops the clock, `q` quits.
fn spawn_stdin_transport_control() {
    std::thread::spawn(|| {
        let mut line = String::new();
        loop {
            line.clear();
            if std::io::stdin().read_line(&mut line).is_err() {
                return;
            }
            match line.trim() {
                "q" | "quit" | "exit" => std::process::exit(0),
                _ => {
                    if TRANSPORT_CMD_CHANNEL.try_send(TransportCmd::Toggle).is_ok() {
                        log::info!("Transport toggled (Enter to toggle again, q to quit)");
                    }
                }
            }
        }
    });
}

#[embassy_executor::task]
async fn clock_engine() {
    run_unified_clock_engine().await
}

/// The layout spawn loop — the simulator's equivalent of the firmware's
/// core-1 main task.
#[embassy_executor::task]
async fn layout_loop(spawner: Spawner) {
    let lm = LAYOUT_MANAGER.init(LayoutManager::new(spawner));
    let mut receiver = LAYOUT_WATCH.receiver().unwrap();
    loop {
        match select(receiver.changed(), FORCE_RESPAWN_SIGNAL.wait()).await {
            Either::First(layout) => {
                if lm.spawn_layout(&layout).await {
                    store_layout(&layout).await;
                }
            }
            Either::Second(_) => {
                let layout = receiver.get().await;
                lm.respawn_all(&layout).await;
            }
        }
    }
}

/// Mirrors the firmware's boot sequence in `main.rs`, minus the hardware.
#[embassy_executor::task]
async fn boot(spawner: Spawner, ports: midi::SimMidiPorts) {
    spawner.spawn(storage::run_storage(fram_path())).unwrap();

    migrate_fram().await;

    let global_config = load_global_config().await;
    GLOBAL_CONFIG_WATCH.sender().send(global_config);

    state::init_state().await;

    spawner.spawn(hw::run_virtual_max()).unwrap();
    spawner.spawn(hw::run_leds()).unwrap();

    fp_core::tasks::input_handlers::start_input_handlers(&spawner).await;
    fp_core::tasks::global_config::start_global_config(&spawner).await;

    spawner.spawn(clock_engine()).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
    spawner.spawn(metronome()).unwrap();

    spawner.spawn(midi_distributor()).unwrap();
    spawner
        .spawn(midi::midi_out_bridge(ports.perf_out))
        .unwrap();
    spawner.spawn(midi::midi_in_bridge()).unwrap();
    spawner.spawn(midi::config_in_bridge()).unwrap();
    spawner.spawn(midi::config_loop(ports.config_out)).unwrap();

    spawner.spawn(hw::dac_monitor()).unwrap();
    spawner.spawn(layout_loop(spawner)).unwrap();

    let mut layout = load_layout().await;

    // PoC helper: force the LFO app (id 2) onto channel 0
    if std::env::var_os("FP_SIM_LFO").is_some() {
        layout.0[0] = Some((2, 1, 0));
        log::info!("FP_SIM_LFO set: LFO app forced onto channel 0");
    }

    log::info!(
        "Booted. {} app(s) in layout, internal BPM {}",
        layout.count(),
        get_bpm()
    );
    log::info!("Press Enter to start/stop the clock transport, q+Enter to quit");

    LAYOUT_WATCH.sender().send(layout);
}

fn get_bpm() -> f32 {
    fp_core::tasks::global_config::get_global_config()
        .clock
        .internal_bpm
}
