use core::ops::Range;

use at24cx::{At24Cx, PAGE_SIZE};
use config::Layout;
use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_storage_async::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};
use heapless::{FnvIndexMap, Vec};
use sequential_storage::{
    cache::NoCache,
    map::{fetch_all_items, store_item},
};

use crate::{
    storage::{AppStorageCmd, AppStoragePublisher, APP_STORAGE_CMD_PUBSUB},
    HardwareEvent, CONFIG_CHANGE_WATCH, EVENT_PUBSUB,
};

type KeyType = u16;
type Address = u32;
type Eeprom = At24Cx<I2c<'static, I2C1, Async>, Delay>;

const RESERVED_PAGES: usize = 32;
const RESERVED_BYTES: usize = RESERVED_PAGES * PAGE_SIZE;
// These are the flash addresses in which the sequential_storage will operate.
const FLASH_RANGE: Range<u32> = 0x0000..(0x20_000 - RESERVED_BYTES as u32);

const MAX_PENDING_SAVES: usize = 16;

// TODO: Find a good number for this (allowed storage size is 64)
pub const DATA_LENGTH: usize = 128;

pub type EepromData = Vec<u8, DATA_LENGTH>;

pub static EEPROM_CHANNEL: Channel<ThreadModeRawMutex, (AppStorageKey, EepromData), 16> =
    Channel::new();

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StorageSlotType {
    Param,
    Storage,
}

#[derive(Clone, Copy)]
pub struct AppStorageKey {
    pub start_channel: u8,
    pub storage_slot: u8,
    pub scene: Option<u8>,
    pub slot_type: StorageSlotType,
}

impl From<AppStorageKey> for u16 {
    fn from(key: AppStorageKey) -> Self {
        const START_CHANNEL_MASK: u8 = 0b1111; // 4 bits
        const STORAGE_SLOT_MASK: u8 = 0b1111; // 4 bits
        const SCENE_MASK: u8 = 0b1111; // 4 bits

        let masked_start_channel = key.start_channel & START_CHANNEL_MASK;
        let masked_storage_slot = key.storage_slot & STORAGE_SLOT_MASK;
        let masked_scene = key.scene.map_or(0, |sc| sc & SCENE_MASK);
        let scene_flag = key.scene.is_some() as u16;
        let slot_type_value = key.slot_type as u16; // Uses #[repr(u8)] value

        ((slot_type_value) << 13) // Bits 13-14 for slot type
        | (scene_flag << 12)        // Bit 12 for scene flag
        | ((masked_scene as u16) << 8)  // Bits 8-11 for scene
        | ((masked_storage_slot as u16) << 4) // Bits 4-7 for storage slot
        | (masked_start_channel as u16) // Bits 0-3 for start channel
    }
}

impl TryFrom<u16> for AppStorageKey {
    type Error = StorageError;
    /// Parses a 16-bit storage key into its components.
    fn try_from(key: u16) -> Result<Self, Self::Error> {
        const START_CHANNEL_MASK: u16 = 0b0000_0000_0000_1111;
        const STORAGE_SLOT_MASK: u16 = 0b0000_0000_1111_0000;
        const SCENE_MASK: u16 = 0b0000_1111_0000_0000;
        const SCENE_FLAG_MASK: u16 = 0b0001_0000_0000_0000;
        const SLOT_TYPE_MASK: u16 = 0b0110_0000_0000_0000;

        const STORAGE_SLOT_SHIFT: u32 = 4;
        const SCENE_SHIFT: u32 = 8;
        const SCENE_FLAG_SHIFT: u32 = 12;
        const SLOT_TYPE_SHIFT: u32 = 13;

        let start_channel = (key & START_CHANNEL_MASK) as u8;
        let storage_slot = ((key & STORAGE_SLOT_MASK) >> STORAGE_SLOT_SHIFT) as u8;
        let scene_value = ((key & SCENE_MASK) >> SCENE_SHIFT) as u8;
        let is_scene_specific = ((key & SCENE_FLAG_MASK) >> SCENE_FLAG_SHIFT) == 1;
        let slot_type_value = ((key & SLOT_TYPE_MASK) >> SLOT_TYPE_SHIFT) as u8;

        let scene = if is_scene_specific {
            Some(scene_value)
        } else {
            None
        };

        let slot_type = match slot_type_value {
            0 => StorageSlotType::Param,
            1 => StorageSlotType::Storage,
            _ => {
                return Err(StorageError::Key);
            }
        };

        Ok(Self {
            start_channel,
            storage_slot,
            scene,
            slot_type,
        })
    }
}

impl AppStorageKey {
    pub fn new(
        start_channel: u8,
        storage_slot: u8,
        scene: Option<u8>,
        slot_type: StorageSlotType,
    ) -> Self {
        Self {
            start_channel,
            storage_slot,
            scene,
            slot_type,
        }
    }
}

#[derive(Debug)]
pub enum RamEepromError {
    OutOfBounds,
}

impl NorFlashError for RamEepromError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            RamEepromError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
        }
    }
}

struct RamEeprom {
    store: [u8; FLASH_RANGE.end as usize],
}

impl RamEeprom {
    pub fn new() -> Self {
        Self {
            store: [0xff; FLASH_RANGE.end as usize],
        }
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        self.store.as_mut()
    }
}

impl ErrorType for RamEeprom {
    type Error = RamEepromError;
}

impl ReadNorFlash for RamEeprom {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let end = offset + bytes.len();
        if end > self.store.len() {
            Err(RamEepromError::OutOfBounds)
        } else {
            bytes.copy_from_slice(&self.store[offset..end]);
            Ok(())
        }
    }

    fn capacity(&self) -> usize {
        FLASH_RANGE.end as usize
    }
}

impl NorFlash for RamEeprom {
    const WRITE_SIZE: usize = 1;

    const ERASE_SIZE: usize = PAGE_SIZE;

    async fn erase(&mut self, _from: u32, _to: u32) -> Result<(), Self::Error> {
        // Dummy
        Ok(())
    }

    async fn write(&mut self, mut _offset: u32, mut _bytes: &[u8]) -> Result<(), Self::Error> {
        // Dummy
        Ok(())
    }
}

pub struct Storage {
    /// eeprom driver
    eeprom: Eeprom,
    /// partition in EEPROM
    range: Range<Address>,
    /// Comms for app storage slots
    app_publisher: AppStoragePublisher,
}

#[derive(Debug)]
pub enum StorageError {
    /// underlying I²C error
    Eeprom,
    /// could not parse key bytes (should never happen)
    Key,
    /// sequential_storage error
    SeqStorage,
}

impl Storage {
    pub fn new(eeprom: Eeprom, range: Range<Address>, app_publisher: AppStoragePublisher) -> Self {
        Self {
            eeprom,
            range,
            app_publisher,
        }
    }

    pub async fn chip_erase(&mut self) -> Result<(), at24cx::Error<impl core::fmt::Debug>> {
        // 0xFF = erased
        let page: [u8; PAGE_SIZE] = [0xFF; PAGE_SIZE];

        let capacity = self.eeprom.capacity() as u32;
        let mut offset = 0u32;

        while offset < capacity {
            self.eeprom.write(offset, &page).await?;
            offset += PAGE_SIZE as u32;
        }
        Ok(())
    }

    pub async fn refresh(&mut self, layout: Layout) -> usize {
        info!("READING ALL OF EEPROM...");
        let mut start_time = Instant::now();
        let mut ram_eeprom = RamEeprom::new();
        self.eeprom.read(0, ram_eeprom.as_mut()).await.unwrap();
        let dur_read_all = Instant::now() - start_time;

        info!("READ DONE. Took {}ms", dur_read_all.as_millis());

        info!("PROCESSING ITEMS");
        start_time = Instant::now();

        let mut count: usize = 0;
        let mut buf = [0; PAGE_SIZE];
        if let Ok(mut all_items_iter) = fetch_all_items::<u16, _, _>(
            &mut ram_eeprom,
            self.range.clone(),
            &mut NoCache::new(),
            &mut buf,
        )
        .await
        {
            let mut item_buf = [0; DATA_LENGTH];
            while let Ok(Some((raw_key, value))) =
                all_items_iter.next::<u16, &[u8]>(&mut item_buf).await
            {
                if let Ok(key @ AppStorageKey { .. }) = raw_key.try_into() {
                    let stored_app_id = value[0];

                    if layout.iter().any(|(app_id, start_channel)| {
                        (*start_channel as u8) == key.start_channel && *app_id == stored_app_id
                    }) {
                        let data = Vec::from_slice(value).unwrap();
                        count += 1;
                        self.app_publisher
                            .publish(AppStorageCmd::ReadAppStorageSlot { key, data })
                            .await;
                    }
                }
            }
        }
        let dur_process = Instant::now() - start_time;
        info!(
            "PROCESSING DONE. Took {}ms for {} items",
            dur_process.as_millis(),
            count
        );
        count
    }

    pub async fn store(&mut self, key: KeyType, value: &[u8]) -> Result<(), StorageError> {
        let mut buf = [0; PAGE_SIZE];

        store_item(
            &mut self.eeprom,
            self.range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &key,
            &value,
        )
        .await
        .map_err(|_| StorageError::SeqStorage)
    }
}

pub async fn start_eeprom(spawner: &Spawner, eeprom: At24Cx<I2c<'static, I2C1, Async>, Delay>) {
    spawner.spawn(run_eeprom(eeprom)).unwrap();
}

struct PendingSave {
    last_update: Instant,
    data: Vec<u8, DATA_LENGTH>,
}

#[embassy_executor::task]
async fn run_eeprom(eeprom: Eeprom) {
    let app_publisher = APP_STORAGE_CMD_PUBSUB.publisher().unwrap();
    let mut config_change_receiver = CONFIG_CHANGE_WATCH.receiver().unwrap();

    // Map to store pending saves: key -> (timestamp, data)
    let mut pending_saves: FnvIndexMap<KeyType, PendingSave, MAX_PENDING_SAVES> =
        FnvIndexMap::new();
    // Debounce duration
    let debounce_duration = Duration::from_secs(1);

    let mut storage = Storage::new(eeprom, FLASH_RANGE, app_publisher);

    loop {
        // Calculate the earliest deadline among pending saves
        let earliest_deadline = pending_saves
            .values()
            .map(|p| p.last_update + debounce_duration)
            .min();

        // Create a timer future that fires at the earliest deadline, or never if no saves are pending
        let timer_future = match earliest_deadline {
            Some(deadline) => Timer::at(deadline),
            // Effectively wait forever if no saves pending
            None => Timer::after(Duration::from_secs(3600)),
        };

        match select3(
            EEPROM_CHANNEL.receive(),
            config_change_receiver.changed(),
            timer_future,
        )
        .await
        {
            Either3::First((key, data)) => {
                let now = Instant::now();
                let pending_save = PendingSave {
                    last_update: now,
                    data,
                };
                pending_saves.insert(key.into(), pending_save).ok();
            }

            Either3::Second(config) => {
                storage.refresh(config.layout).await;
                let event_publisher = EVENT_PUBSUB.publisher().unwrap();
                event_publisher.publish(HardwareEvent::EepromRefresh).await;
            }

            Either3::Third(_) => {
                let now = Instant::now();
                let mut keys_to_save: Vec<u16, MAX_PENDING_SAVES> = Vec::new();

                for (key, pending) in pending_saves.iter() {
                    if now >= pending.last_update + debounce_duration {
                        keys_to_save.push(*key).ok();
                    }
                }

                for key in keys_to_save {
                    if let Some(pending) = pending_saves.get(&key) {
                        storage.store(key, pending.data.as_slice()).await.unwrap();
                    }
                    pending_saves.remove(&key);
                }
            }
        }
    }
}
