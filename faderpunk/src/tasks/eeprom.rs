use at24cx::At24Cx;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver};
use embassy_time::{Delay, Duration, Instant, Timer};
use heapless::{FnvIndexMap, Vec};
use sequential_storage::{
    cache::NoCache,
    map::{fetch_item, store_item},
};

use crate::{XTxMsg, XTxSender};

pub type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, StorageMsg), 64>;

pub const DATA_LENGTH: usize = 65;
const MAX_PENDING_SAVES: usize = 64;

#[derive(Clone)]
pub enum StorageMsg {
    Read(u8, u8, Vec<u8, DATA_LENGTH>),
    Request(u8, u8),
    Store(u8, u8, Vec<u8, DATA_LENGTH>),
}

pub async fn start_eeprom(
    spawner: &Spawner,
    eeprom: At24Cx<I2c<'static, I2C1, Async>, Delay>,
    sender: XTxSender,
    receiver: XRxReceiver,
) {
    spawner.spawn(run_eeprom(eeprom, sender, receiver)).unwrap();
}

fn create_storage_key(app_id: u8, start_channel: u8, storage_slot: u8) -> u32 {
    ((app_id as u32) << 9) | ((storage_slot as u32) << 4) | (start_channel as u32)
}

// Helper struct to store pending save info
struct PendingSave {
    last_update: Instant,
    data: Vec<u8, DATA_LENGTH>,
    app_id: u8,        // Keep for potential logging/debugging
    storage_slot: u8,  // Keep for potential logging/debugging
    start_channel: u8, // Keep for potential logging/debugging
}

#[embassy_executor::task]
async fn run_eeprom(
    mut eeprom: At24Cx<I2c<'static, I2C1, Async>, Delay>,
    sender: XTxSender,
    receiver: XRxReceiver,
) {
    // These are the flash addresses in which the crate will operate.
    // The crate will not read, write or erase outside of this range.
    // Total EEPROM size: 128KB (0x20000 bytes)
    // Reserved space: 32KB (0x8000 bytes)
    // Usable range: 0x8000 to 0x20000
    let flash_range = 0x8000..0x20000;
    // We need to give the crate a buffer to work with.
    // It must be big enough to serialize the biggest value of your storage type in,
    // rounded up to to word alignment of the flash. Some kinds of internal flash may require
    // this buffer to be aligned in RAM as well.
    let mut data_buffer = [0; 128];

    // Map to store pending saves: key -> (timestamp, data)
    let mut pending_saves: FnvIndexMap<u32, PendingSave, MAX_PENDING_SAVES> = FnvIndexMap::new();
    // Debounce duration
    let debounce_duration = Duration::from_secs(1);

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
        match select(receiver.receive(), timer_future).await {
            Either::First(msg) => match msg {
                (chan, StorageMsg::Request(app_id, storage_slot)) => {
                    let key = create_storage_key(app_id, chan as u8, storage_slot);
                    if let Ok(Some(item)) = fetch_item::<u32, &[u8], _>(
                        &mut eeprom,
                        flash_range.clone(),
                        &mut NoCache::new(),
                        &mut data_buffer,
                        &key,
                    )
                    .await
                    {
                        if let Ok(vec) = Vec::<u8, DATA_LENGTH>::from_slice(item) {
                            sender
                                .send((
                                    chan,
                                    XTxMsg::StorageMsg(StorageMsg::Read(app_id, storage_slot, vec)),
                                ))
                                .await;
                        }
                    }
                }
                (chan, StorageMsg::Store(app_id, storage_slot, data)) => {
                    let key = create_storage_key(app_id, chan as u8, storage_slot);
                    let now = Instant::now();
                    let pending_save = PendingSave {
                        last_update: now,
                        data,
                        app_id,
                        storage_slot,
                        start_channel: chan as u8,
                    };
                    pending_saves.insert(key, pending_save).ok();
                }
                _ => {}
            },

            Either::Second(_) => {
                let now = Instant::now();
                let mut keys_to_save: Vec<u32, MAX_PENDING_SAVES> = Vec::new();

                for (key, pending) in pending_saves.iter() {
                    if now >= pending.last_update + debounce_duration {
                        keys_to_save.push(*key).ok();
                    }
                }

                for key in keys_to_save {
                    if let Some(pending) = pending_saves.get(&key) {
                        store_item(
                            &mut eeprom,
                            flash_range.clone(),
                            &mut NoCache::new(),
                            &mut data_buffer,
                            &key,
                            &pending.data.as_slice(),
                        )
                        .await
                        .ok();
                    }
                    pending_saves.remove(&key);
                }
            }
        }
    }
}
