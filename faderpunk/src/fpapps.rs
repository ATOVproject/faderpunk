//! RP2350 flash adapter and global store for installable FPApps.

use core::slice;

use embassy_rp::flash::{Blocking, Error, Flash, FLASH_BASE};
use embassy_rp::peripherals::FLASH;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::once_lock::OnceLock;
use libfp::fpapp_store::{SlotFlash, SlotStore, ERASE_SIZE, FPAPP_REGION_SIZE};
use portable_atomic::{AtomicU32, AtomicU8, Ordering};

use crate::version::FPAPP_FIRMWARE_ABI;
use crate::watchdog;

const PHYSICAL_FLASH_SIZE: usize = 2 * 1024 * 1024;
const REGION_BASE: usize = PHYSICAL_FLASH_SIZE - FPAPP_REGION_SIZE;

// Ensure FPApp region base address does not overlap main firmware FLASH allocation in memory.x (1536 KiB)
const _: () = assert!(
    REGION_BASE >= 1536 * 1024,
    "FPApp region base conflicts with memory.x FLASH size!"
);
#[derive(Clone, Copy)]
pub struct RuntimeDescriptor {
    /// Which slot this came from. Carried so the runtime can name the culprit
    /// in the watchdog marker before handing control to its native code.
    pub slot: u8,
    pub app_id: u8,
    pub channels: u8,
    pub code_base: u32,
    pub required_bytes: u32,
    pub init: u32,
    pub poll: u32,
    pub drop: u32,
}

struct CachedDescriptor {
    // Published last with Release ordering; readers acquire it before loading
    // the remaining fields.
    app_id: AtomicU8,
    channels: AtomicU8,
    code_base: AtomicU32,
    required_bytes: AtomicU32,
    init: AtomicU32,
    poll: AtomicU32,
    drop: AtomicU32,
}

impl CachedDescriptor {
    const fn new() -> Self {
        Self {
            app_id: AtomicU8::new(0),
            channels: AtomicU8::new(0),
            code_base: AtomicU32::new(0),
            required_bytes: AtomicU32::new(0),
            init: AtomicU32::new(0),
            poll: AtomicU32::new(0),
            drop: AtomicU32::new(0),
        }
    }

    fn clear(&self) {
        self.app_id.store(0, Ordering::Release);
    }

    fn publish(&self, descriptor: RuntimeDescriptor) {
        self.clear();
        self.channels.store(descriptor.channels, Ordering::Relaxed);
        self.code_base
            .store(descriptor.code_base, Ordering::Relaxed);
        self.required_bytes
            .store(descriptor.required_bytes, Ordering::Relaxed);
        self.init.store(descriptor.init, Ordering::Relaxed);
        self.poll.store(descriptor.poll, Ordering::Relaxed);
        self.drop.store(descriptor.drop, Ordering::Relaxed);
        self.app_id.store(descriptor.app_id, Ordering::Release);
    }

    fn get(&self, slot: usize, app_id: u8) -> Option<RuntimeDescriptor> {
        if self.app_id.load(Ordering::Acquire) != app_id {
            return None;
        }
        Some(RuntimeDescriptor {
            slot: slot as u8,
            app_id,
            channels: self.channels.load(Ordering::Relaxed),
            code_base: self.code_base.load(Ordering::Relaxed),
            required_bytes: self.required_bytes.load(Ordering::Relaxed),
            init: self.init.load(Ordering::Relaxed),
            poll: self.poll.load(Ordering::Relaxed),
            drop: self.drop.load(Ordering::Relaxed),
        })
    }
}

static RUNTIME_DESCRIPTORS: [CachedDescriptor; 4] = [const { CachedDescriptor::new() }; 4];

pub struct RpFpAppFlash<'d> {
    flash: Flash<'d, FLASH, Blocking, PHYSICAL_FLASH_SIZE>,
}

impl<'d> RpFpAppFlash<'d> {
    fn new(peripheral: Peri<'d, FLASH>) -> Self {
        Self {
            flash: Flash::new_blocking(peripheral),
        }
    }
}

impl SlotFlash for RpFpAppFlash<'_> {
    type Error = Error;

    fn len(&self) -> usize {
        FPAPP_REGION_SIZE
    }

    fn mapped(&self, offset: usize, len: usize) -> Result<&[u8], Self::Error> {
        let end = offset.checked_add(len).ok_or(Error::OutOfBounds)?;
        if end > FPAPP_REGION_SIZE {
            return Err(Error::OutOfBounds);
        }
        let address = FLASH_BASE as usize + REGION_BASE + offset;
        // SAFETY: The bounds check above keeps this slice inside the physical
        // flash region permanently reserved for FPApps by memory.x.
        Ok(unsafe { slice::from_raw_parts(address as *const u8, len) })
    }

    fn erase(&mut self, offset: usize, len: usize) -> Result<(), Self::Error> {
        let start = REGION_BASE.checked_add(offset).ok_or(Error::OutOfBounds)?;
        let end = start.checked_add(len).ok_or(Error::OutOfBounds)?;
        if end > PHYSICAL_FLASH_SIZE {
            return Err(Error::OutOfBounds);
        }
        // `blocking_erase` runs the whole range inside one `in_ram` call, which
        // pauses Core 1 *and* holds a critical section on Core 0 — so for its
        // full duration neither core can feed the watchdog. Erasing a slot in
        // one call would be a multi-second blind spot and would reset a device
        // that is merely installing an app. Walk it a sector at a time instead
        // and feed in between, which keeps every blind spot down to a single
        // sector erase and lets the watchdog stay armed at its normal period
        // throughout an install. The step must stay aligned to embassy's own
        // `flash::ERASE_SIZE` or its `check_erase` rejects the range; the two
        // constants agree at 4096.
        let mut sector = start;
        while sector < end {
            let sector_end = sector.saturating_add(ERASE_SIZE).min(end);
            self.flash
                .blocking_erase(sector as u32, sector_end as u32)?;
            watchdog::feed();
            sector = sector_end;
        }
        Ok(())
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error> {
        let start = REGION_BASE.checked_add(offset).ok_or(Error::OutOfBounds)?;
        let end = start.checked_add(data.len()).ok_or(Error::OutOfBounds)?;
        if end > PHYSICAL_FLASH_SIZE {
            return Err(Error::OutOfBounds);
        }
        self.flash.blocking_write(start as u32, data)
    }
}

pub type FpAppStore = Mutex<CriticalSectionRawMutex, SlotStore<RpFpAppFlash<'static>>>;

pub static FPAPP_STORE: OnceLock<FpAppStore> = OnceLock::new();

pub fn init(peripheral: Peri<'static, FLASH>) {
    let flash = RpFpAppFlash::new(peripheral);
    let store = SlotStore::open(flash, FPAPP_FIRMWARE_ABI)
        .expect("reserved FPApp flash region must be valid");
    refresh_catalog(&store);
    FPAPP_STORE
        .init(Mutex::new(store))
        .unwrap_or_else(|_| panic!("FPApp store initialized twice"));
}

/// Slots whose app hung the device badly enough to trip the watchdog. Held in
/// RAM so enforcement does not depend on FRAM being readable, and mirrored from
/// `RuntimeState` at boot so it survives a power cycle.
static QUARANTINED_SLOTS: AtomicU8 = AtomicU8::new(0);

/// True if any slot holds an app that is actually allowed to run. Empty and
/// quarantined slots both answer no, since neither can put native code on
/// Core 1 — so a unit whose only app is quarantined needs no watchdog, and
/// becomes reflashable over SWD again straight after a hang.
pub fn has_runnable_app() -> bool {
    RUNTIME_DESCRIPTORS
        .iter()
        .any(|cached| cached.app_id.load(Ordering::Acquire) != 0)
}

pub fn quarantined_slots() -> u8 {
    QUARANTINED_SLOTS.load(Ordering::Relaxed)
}

/// Install the quarantine mask and rebuild the catalog under it.
///
/// `init` runs before the persisted state is loaded, so the first catalog is
/// necessarily built without this. Boot calls it once the mask is known and
/// before the layout ships to Core 1, so a quarantined app never gets spawned.
pub async fn apply_quarantine(mask: u8) {
    QUARANTINED_SLOTS.store(mask, Ordering::Relaxed);
    let store = FPAPP_STORE.get().await;
    refresh_catalog(&*store.lock().await);
}

/// Lift a slot's quarantine because its contents changed — whatever hung the
/// device is no longer what lives there. Call before refreshing the catalog so
/// the refresh republishes the slot. A no-op if it wasn't quarantined.
pub async fn clear_quarantine(slot: usize) {
    let previous = QUARANTINED_SLOTS.load(Ordering::Relaxed);
    let mask = previous & !(1u8 << slot);
    if mask == previous {
        return;
    }
    QUARANTINED_SLOTS.store(mask, Ordering::Relaxed);
    crate::state::update_state(|state| {
        state.quarantined_slots = mask;
        true
    })
    .await;
}

pub fn refresh_catalog(store: &SlotStore<RpFpAppFlash<'static>>) {
    let quarantined = QUARANTINED_SLOTS.load(Ordering::Relaxed);
    for (slot, cached) in RUNTIME_DESCRIPTORS.iter().enumerate() {
        // Leaving a quarantined slot's descriptor cleared is the whole
        // enforcement mechanism: `runtime_descriptor` then misses on its app id
        // and the layout silently declines to spawn it.
        if quarantined & (1 << slot) != 0 {
            cached.clear();
            continue;
        }
        let descriptor = store.package(slot).ok().flatten().and_then(|package| {
            let native = package.native_program().ok()?;
            Some(RuntimeDescriptor {
                slot: slot as u8,
                app_id: package.manifest.app_id,
                channels: package.manifest.channels,
                code_base: native.image.as_ptr() as u32,
                required_bytes: native.entrypoints.required_bytes,
                init: native.entrypoints.init,
                poll: native.entrypoints.poll,
                drop: native.entrypoints.drop,
            })
        });
        if let Some(descriptor) = descriptor {
            cached.publish(descriptor);
        } else {
            cached.clear();
        }
    }
}

pub fn runtime_descriptor(app_id: u8) -> Option<RuntimeDescriptor> {
    RUNTIME_DESCRIPTORS
        .iter()
        .enumerate()
        .find_map(|(slot, cached)| cached.get(slot, app_id))
}

pub fn get_channels(app_id: u8) -> Option<usize> {
    runtime_descriptor(app_id).map(|descriptor| descriptor.channels as usize)
}
