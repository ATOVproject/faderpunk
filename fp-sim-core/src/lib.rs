mod hw;
mod ipc;
mod midi;
mod panel;
mod storage;

use std::path::PathBuf;

use embassy_executor::{Executor, Spawner};
use embassy_futures::select::{select3, Either3};
use portable_atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;

use fp_core::layout::{
    EvictionCmd, LayoutManager, FORCE_RESPAWN_SIGNAL, LAYOUT_EVICTION_REQ, LAYOUT_EVICTION_RES,
    LAYOUT_MANAGER, LAYOUT_WATCH,
};
use fp_core::registry::ExternalAppDescriptor;
use fp_core::storage::{load_global_config, load_layout, migrate_fram, store_layout};
use fp_core::tasks::clock::{metronome, run_clock_gatekeeper, run_unified_clock_engine};
use fp_core::tasks::global_config::GLOBAL_CONFIG_WATCH;
use fp_core::tasks::midi::midi_distributor;
use fp_core::{apps, platform, state};
use fp_sim_protocol::CoreToHost;

include!(concat!(env!("OUT_DIR"), "/firmware_version.rs"));

static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static RNG_STATE: AtomicU32 = AtomicU32::new(0);

pub fn run(external_apps: &'static [ExternalAppDescriptor]) -> ! {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    apps::set_external_apps(external_apps);
    ipc::init();
    platform::init(platform::Platform {
        rand_u16,
        sys_reset,
    });

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(boot(spawner)).unwrap();
    });
}

fn rand_u16() -> u16 {
    let mut value = RNG_STATE.load(Ordering::Relaxed);
    while value == 0 {
        value = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0x1234_5678)
            | 1;
    }
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    RNG_STATE.store(value, Ordering::Relaxed);
    (value >> 8) as u16
}

fn sys_reset() -> ! {
    log::info!("System reset requested — exiting for host restart");
    std::process::exit(0)
}

fn fram_path() -> PathBuf {
    std::env::var_os("FP_SIM_FRAM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fp-sim-fram.bin"))
}

#[embassy_executor::task]
async fn clock_engine() {
    run_unified_clock_engine().await
}

#[embassy_executor::task]
async fn layout_loop(spawner: Spawner) {
    let manager = LAYOUT_MANAGER.init(LayoutManager::new(spawner));
    let mut receiver = LAYOUT_WATCH.receiver().unwrap();
    loop {
        match select3(
            receiver.changed(),
            FORCE_RESPAWN_SIGNAL.wait(),
            LAYOUT_EVICTION_REQ.wait(),
        )
        .await
        {
            Either3::First(layout) => {
                if manager.spawn_layout(&layout).await {
                    store_layout(&layout).await;
                }
            }
            Either3::Second(_) => {
                let layout = receiver.get().await;
                manager.respawn_all(&layout).await;
            }
            Either3::Third(command) => {
                match command {
                    EvictionCmd::Evict(start_channel) => {
                        manager.set_held(start_channel, true).await;
                        manager.exit_app(start_channel).await;
                    }
                    EvictionCmd::Restore(start_channel, app_id, channels, layout_id) => {
                        manager
                            .spawn_one(start_channel, app_id, channels, layout_id)
                            .await;
                        manager.set_held(start_channel, false).await;
                    }
                }
                LAYOUT_EVICTION_RES.signal(());
            }
        }
    }
}

#[embassy_executor::task]
async fn boot(spawner: Spawner) {
    spawner.spawn(storage::run_storage(fram_path())).unwrap();
    migrate_fram().await;

    GLOBAL_CONFIG_WATCH
        .sender()
        .send(load_global_config().await);
    state::init_state().await;

    spawner.spawn(hw::run_virtual_max()).unwrap();
    spawner.spawn(hw::run_leds()).unwrap();
    spawner.spawn(panel::run_buttons()).unwrap();
    spawner.spawn(panel::run_faders()).unwrap();

    fp_core::tasks::input_handlers::start_input_handlers(&spawner).await;
    fp_core::tasks::global_config::start_global_config(&spawner).await;

    spawner.spawn(clock_engine()).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
    spawner.spawn(metronome()).unwrap();

    spawner.spawn(midi_distributor()).unwrap();
    spawner.spawn(midi::midi_out_bridge()).unwrap();
    spawner.spawn(midi::midi_in_bridge()).unwrap();
    spawner.spawn(midi::config_in_bridge()).unwrap();
    spawner.spawn(midi::config_loop()).unwrap();
    spawner.spawn(layout_loop(spawner)).unwrap();

    if std::env::var_os("FP_SIM_MONITOR").is_some() {
        spawner.spawn(hw::dac_monitor()).unwrap();
    }

    let mut layout = load_layout().await;
    if let Some(app_id) = forced_app_id() {
        if let Some(channels) = apps::get_channels(app_id) {
            layout.0.fill(None);
            layout.0[0] = Some((app_id, channels, 0));
            log::info!("Forced app {app_id} onto channel 0");
        } else {
            log::warn!("Ignoring unknown FP_SIM_APP_ID {app_id}");
        }
    }

    log::info!(
        "Core booted. {} app(s) in layout, internal BPM {}",
        layout.count(),
        fp_core::tasks::global_config::get_global_config()
            .clock
            .internal_bpm
    );
    LAYOUT_WATCH.sender().send(layout);
    spawner.spawn(ipc::publish_state()).unwrap();
    ipc::send(CoreToHost::Ready {
        firmware_version: FIRMWARE_VERSION,
    });
}

fn forced_app_id() -> Option<u8> {
    std::env::var("FP_SIM_APP_ID")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| std::env::var_os("FP_SIM_LFO").map(|_| 2))
}
