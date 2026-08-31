//! RP2350 flash adapter and global store for installable FPApps.

use core::slice;

use embassy_rp::flash::{Blocking, Error, Flash, FLASH_BASE};
use embassy_rp::peripherals::FLASH;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::once_lock::OnceLock;
use libfp::fpapp_store::{SlotFlash, SlotStore, FPAPP_REGION_SIZE};
use portable_atomic::{AtomicU32, AtomicU8, Ordering};

use crate::version::FPAPP_FIRMWARE_ABI;

const PHYSICAL_FLASH_SIZE: usize = 2 * 1024 * 1024;
const REGION_BASE: usize = PHYSICAL_FLASH_SIZE - FPAPP_REGION_SIZE;

// Ensure FPApp region base address does not overlap main firmware FLASH allocation in memory.x (1536 KiB)
const _: () = assert!(
    REGION_BASE >= 1536 * 1024,
    "FPApp region base conflicts with memory.x FLASH size!"
);
#[derive(Clone, Copy)]
pub struct RuntimeDescriptor {
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

    fn get(&self, app_id: u8) -> Option<RuntimeDescriptor> {
        if self.app_id.load(Ordering::Acquire) != app_id {
            return None;
        }
        Some(RuntimeDescriptor {
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
        self.flash.blocking_erase(start as u32, end as u32)
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

pub fn refresh_catalog(store: &SlotStore<RpFpAppFlash<'static>>) {
    for (slot, cached) in RUNTIME_DESCRIPTORS.iter().enumerate() {
        let descriptor = store.package(slot).ok().flatten().and_then(|package| {
            let native = package.native_program().ok()?;
            Some(RuntimeDescriptor {
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
        .find_map(|descriptor| descriptor.get(app_id))
}

pub fn get_channels(app_id: u8) -> Option<usize> {
    runtime_descriptor(app_id).map(|descriptor| descriptor.channels as usize)
}
