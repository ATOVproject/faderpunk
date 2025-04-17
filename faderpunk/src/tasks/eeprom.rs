use core::mem::size_of;
use core::ops::Range;

use at24cx::{At24Cx, PAGE_SIZE};
use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_storage_async::nor_flash::ReadNorFlash;
use heapless::{FnvIndexMap, Vec};
use sequential_storage::{cache::NoCache, map::store_item};

use crate::storage::{
    create_storage_key, StorageCmd, StorageEvent, DATA_LENGTH, STORAGE_CMD_CHANNEL,
    STORAGE_EVENT_PUBSUB,
};

type Key = u16;
type Address = u32;

// key size (2) + len(2) + crc(2)
const HEADER_LEN: usize = size_of::<Key>() + 4;
// 16 concurrent apps, 8 storage slots each, 16 scenes each and one value (plus some headroom)
const MAX_ITEMS: usize = 16 * 8 * (16 + 1) + 32;

const RESERVED_PAGES: usize = 32;
const RESERVED_BYTES: usize = RESERVED_PAGES * PAGE_SIZE;

const MAX_PENDING_SAVES: usize = 16;

type Eeprom = At24Cx<I2c<'static, I2C1, Async>, Delay>;

pub struct Storage {
    /// eeprom driver
    eeprom: Eeprom,
    /// partition in EEPROM
    range: Range<Address>,
    /// first free byte after the log
    next_addr: Address,
    /// storage index
    index: FnvIndexMap<Key, Address, MAX_ITEMS>,
    // shared scratch buffer
    buf: [u8; PAGE_SIZE],
}

const fn align4(x: u32) -> u32 {
    (x + 3) & !3
}

#[derive(Debug)]
pub enum StorageError {
    /// underlying I²C error
    Eeprom,
    /// could not parse key bytes (should never happen)
    Key,
    /// could not parse len bytes
    Length,
    /// len field would make the record cross the partition end
    RecordTooLong,
    /// FnvIndexMap is full
    IndexOverflow,
    // caller buffer too small
    BufferOverflow,
    // sequential_storage error
    SeqStorage,
}

impl Storage {
    pub fn new(eeprom: Eeprom, range: Range<Address>) -> Self {
        Self {
            eeprom,
            next_addr: range.start,
            range,
            index: FnvIndexMap::new(),
            buf: [0; PAGE_SIZE],
        }
    }

    /// walk once through the log, fill the index
    pub async fn scan(&mut self) -> Result<(), StorageError> {
        loop {
            if self.next_addr + HEADER_LEN as u32 > self.range.end {
                // stop if less than a header fits in the partition
                return Ok(());
            }

            self.eeprom
                .read(self.next_addr, &mut self.buf[..HEADER_LEN])
                .await
                .map_err(|_| StorageError::Eeprom)?;

            let h = &self.buf[..HEADER_LEN];

            if h.iter().all(|&b| b == 0xFF) {
                // scan complete
                return Ok(());
            }

            let key = Key::from_le_bytes(
                h[0..size_of::<Key>()]
                    .try_into()
                    .map_err(|_| StorageError::Key)?,
            );
            let len = u16::from_le_bytes(
                h[size_of::<Key>()..size_of::<Key>() + 2]
                    .try_into()
                    .map_err(|_| StorageError::Length)?,
            ) as u32;

            let total = HEADER_LEN as u32 + len;

            if self.next_addr + total > self.range.end {
                return Err(StorageError::RecordTooLong);
            }

            self.index
                .insert(key, self.next_addr)
                .map_err(|_| StorageError::IndexOverflow)?;

            self.next_addr += align4(total);
        }
    }

    pub async fn read<'b>(
        &mut self,
        key: Key,
        dst: &'b mut [u8],
    ) -> Result<Option<&'b [u8]>, StorageError> {
        let &addr = match self.index.get(&key) {
            Some(a) => a,
            None => return Ok(None),
        };

        self.eeprom
            .read(addr, &mut self.buf[..HEADER_LEN])
            .await
            .map_err(|_| StorageError::Eeprom)?;

        let len_offset = core::mem::size_of::<Key>();
        let len =
            u16::from_le_bytes(self.buf[len_offset..len_offset + 2].try_into().unwrap()) as usize;

        if len > dst.len() {
            return Err(StorageError::BufferOverflow);
        }

        self.eeprom
            .read(addr + HEADER_LEN as u32, &mut dst[..len])
            .await
            .map_err(|_| StorageError::Eeprom)?;

        Ok(Some(&dst[..len]))
    }

    pub async fn store(&mut self, key: Key, data: &[u8]) -> Result<(), StorageError> {
        let addr_of_new = self.next_addr;

        store_item(
            &mut self.eeprom,
            self.range.clone(),
            &mut NoCache::new(),
            &mut self.buf,
            &key,
            &data,
        )
        .await
        .map_err(|_| StorageError::SeqStorage)?;

        self.index
            .insert(key, addr_of_new)
            .map_err(|_| StorageError::IndexOverflow)?;
        self.next_addr = addr_of_new + align4(HEADER_LEN as u32 + data.len() as u32);

        Ok(())
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
    let event_sender = STORAGE_EVENT_PUBSUB.publisher().unwrap();

    // These are the flash addresses in which the sequential_storage will operate.
    let flash_range = 0x0000..(0x20_000 - RESERVED_BYTES as u32);

    // Map to store pending saves: key -> (timestamp, data)
    let mut pending_saves: FnvIndexMap<u16, PendingSave, MAX_PENDING_SAVES> = FnvIndexMap::new();
    // Debounce duration
    let debounce_duration = Duration::from_secs(1);

    let mut storage = Storage::new(eeprom, flash_range);
    let mut data_buffer: [u8; 256] = [0; 256];

    storage.scan().await.unwrap();

    loop {
        // Calculate the earliest deadline among pending saves
        let earliest_deadline = pending_saves
            .values()
            .map(|p| p.last_update + debounce_duration)
            .min();

        // Create a timer future that fires at the earliest deadline, or never if no saves are pending
        let timer_future = match earliest_deadline {
            Some(deadline) => Timer::at(deadline),
            None => Timer::after(Duration::from_secs(3600)), // Effectively wait forever if no saves pending
        };

        // Wait for either a new message or the timer to expire
        match select(STORAGE_CMD_CHANNEL.receive(), timer_future).await {
            Either::First(msg) => match msg {
                StorageCmd::Request {
                    app_id,
                    start_channel,
                    storage_slot,
                    scene,
                } => {
                    let key = create_storage_key(start_channel, storage_slot, scene);
                    let before = Instant::now();
                    match storage.read(key, &mut data_buffer).await {
                        Ok(Some(item)) => match Vec::<u8, DATA_LENGTH>::from_slice(item) {
                            Ok(vec) => {
                                let duration = Instant::now() - before;
                                info!("LOADED. Took {}ms", duration.as_millis());
                                event_sender
                                    .publish(StorageEvent::Read {
                                        app_id,
                                        start_channel,
                                        storage_slot,
                                        data: vec,
                                        scene,
                                    })
                                    .await;
                            }
                            _ => {
                                let duration = Instant::now() - before;
                                info!("NOT FOUND. Took {}ms", duration.as_millis());
                                event_sender
                                    .publish(StorageEvent::NotFound {
                                        app_id,
                                        start_channel,
                                        storage_slot,
                                        scene,
                                    })
                                    .await;
                            }
                        },
                        _ => {
                            let duration = Instant::now() - before;
                            info!("ERROR. Took {}ms", duration.as_millis());
                            event_sender
                                .publish(StorageEvent::NotFound {
                                    app_id,
                                    start_channel,
                                    storage_slot,
                                    scene,
                                })
                                .await;
                        }
                    }
                }
                StorageCmd::Store {
                    app_id,
                    start_channel,
                    storage_slot,
                    data,
                    scene,
                } => {
                    let key = create_storage_key(start_channel, storage_slot, scene);
                    let now = Instant::now();
                    let pending_save = PendingSave {
                        last_update: now,
                        data,
                    };
                    pending_saves.insert(key, pending_save).ok();
                }
            },

            Either::Second(_) => {
                let now = Instant::now();
                let mut keys_to_save: Vec<u16, MAX_PENDING_SAVES> = Vec::new();

                for (key, pending) in pending_saves.iter() {
                    if now >= pending.last_update + debounce_duration {
                        keys_to_save.push(*key).ok();
                    }
                }

                for key in keys_to_save {
                    if let Some(pending) = pending_saves.get(&key) {
                        storage.store(key, pending.data.as_slice()).await.ok();
                    }
                    pending_saves.remove(&key);
                }
            }
        }
    }
}
