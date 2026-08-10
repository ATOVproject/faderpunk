use core::cell::Cell;

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    signal::Signal,
};
use libfp::ConfigMeta;

pub type ExitSignals = [Signal<embassy_sync::blocking_mutex::raw::NoopRawMutex, bool>; 16];

type SpawnFn = fn(u8, usize, u8, Spawner, &'static ExitSignals);
type ConfigFn = fn() -> ConfigMeta<'static>;

#[derive(Clone, Copy)]
pub struct ExternalAppDescriptor {
    pub id: u8,
    pub channels: usize,
    config: ConfigFn,
    spawn: SpawnFn,
}

impl ExternalAppDescriptor {
    pub const fn new(id: u8, channels: usize, config: ConfigFn, spawn: SpawnFn) -> Self {
        Self {
            id,
            channels,
            config,
            spawn,
        }
    }

    pub fn config(self) -> ConfigMeta<'static> {
        (self.config)()
    }

    pub fn spawn(
        self,
        start_channel: usize,
        layout_id: u8,
        spawner: Spawner,
        exit_signals: &'static ExitSignals,
    ) {
        (self.spawn)(self.id, start_channel, layout_id, spawner, exit_signals);
    }
}

static EXTERNAL_APPS: Mutex<CriticalSectionRawMutex, Cell<&'static [ExternalAppDescriptor]>> =
    Mutex::new(Cell::new(&[]));

pub(crate) fn set_external_apps(apps: &'static [ExternalAppDescriptor]) {
    EXTERNAL_APPS.lock(|registered| registered.set(apps));
}

pub fn external_apps() -> &'static [ExternalAppDescriptor] {
    EXTERNAL_APPS.lock(Cell::get)
}

pub fn external_app(id: u8) -> Option<ExternalAppDescriptor> {
    external_apps().iter().copied().find(|app| app.id == id)
}
