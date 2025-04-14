use config::Param;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    mutex::Mutex,
    watch::Watch,
};
use heapless::Vec;
use postcard::from_bytes;
use serde::{de::DeserializeOwned, Serialize};

// TODO: Find a good number for this (allowed storage size is 64)
pub const DATA_LENGTH: usize = 128;

pub static APP_STORAGE_WATCHES: [Watch<CriticalSectionRawMutex, StorageEvent, 1>; 16] =
    [const { Watch::new() }; 16];

/// Error type for deserializing and storing a value using minicbor
pub enum DeserializeValueError {
    /// Failed to decode the CBOR data into the expected Rust type.
    DeserializationFailed(minicbor::decode::Error),
    /// The provided slot index does not match any configured parameter for this app.
    InvalidSlotIndex,
    // Consider adding other potential errors, e.g., ValidationError
}


#[derive(Clone)]
pub enum StorageEvent {
    Read(u8, u8, Vec<u8, DATA_LENGTH>),
}

#[derive(Clone)]
pub enum StorageCmd {
    Request(u8, u8),
    Store(u8, u8, Vec<u8, DATA_LENGTH>),
}

pub fn create_storage_key(app_id: u8, start_channel: u8, storage_slot: u8) -> u32 {
    ((app_id as u32) << 9) | ((storage_slot as u32) << 4) | (start_channel as u32)
}

pub struct StorageSlot<T: Sized + Copy + Default> {
    pub param: Param,
    pub slot: usize,
    inner: Mutex<ThreadModeRawMutex, T>,
}

// FIXME: START HERE. WE don't want a Param in a StorageSlot. We need to create an AppParam or
// something like that that wraps a StorageSlot
impl<T: Sized + Copy + Default + Serialize + DeserializeOwned> StorageSlot<T> {
    pub const fn new(slot: usize, param: Param, initial: T) -> Self {
        Self {
            param,
            slot,
            inner: Mutex::new(initial),
        }
    }

    pub async fn store(&self, val: T) {
        let mut inner = self.inner.lock().await;
        *inner = val;
    }

    pub async fn des(&self, data: &[u8]) {
        if let Ok(val) = from_bytes::<T>(data) {
            self.store(val).await;
        }
    }

    pub async fn get(&self) -> T {
        let inner = self.inner.lock().await;
        *inner
    }
}
