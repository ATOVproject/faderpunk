use core::marker::PhantomData;

use config::{FromValue, Param, Value};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
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

#[derive(Clone)]
pub enum StorageEvent {
    Read(u8, u8, Vec<u8, DATA_LENGTH>),
}

#[derive(Clone)]
pub enum StorageCmd {
    Request(u8, u8),
    Store(u8, u8, Vec<u8, DATA_LENGTH>),
}

// TODO: For scenes: this has to be adjusted for scenes (we need 4 bits for the scene)
pub fn create_storage_key(app_id: u8, start_channel: u8, storage_slot: u8) -> u32 {
    ((app_id as u32) << 9) | ((storage_slot as u32) << 4) | (start_channel as u32)
}

pub struct ParamStore<const N: usize> {
    inner: Mutex<NoopRawMutex, [Value; N]>,
    change_notifier: Watch<NoopRawMutex, usize, N>,
}

impl<const N: usize> ParamStore<N> {
    /// Creates a new ParamStore, initializing values and the single watcher.
    pub fn new(initial_values: [Value; N]) -> Self {
        // Watch doesn't strictly need an initial value if subscribers wait first.
        // Initialize with a dummy value if preferred, e.g., (usize::MAX, Value::None).
        let change_notifier = Watch::new();
        Self {
            inner: Mutex::new(initial_values),
            change_notifier,
        }
    }

    pub async fn get(&self, index: usize) -> Value {
        let val = self.inner.lock().await;
        val[index]
    }

    pub async fn get_all(&self) -> [Value; N] {
        let val = self.inner.lock().await;
        *val
    }

    pub async fn set(&self, index: usize, value: Value) {
        let mut val = self.inner.lock().await;
        val[index] = value;
        let sender = self.change_notifier.sender();
        sender.send(index);
    }

    pub async fn wait_for_change(&self) -> usize {
        let mut sub = self.change_notifier.receiver().unwrap();
        sub.changed().await
    }
}

pub struct ParamSlot<'a, T, const N: usize>
where
    T: FromValue + Into<Value> + Copy,
{
    // Use the specific Mutex type you have
    values: &'a ParamStore<N>,
    index: usize,
    _phantom: PhantomData<T>,
}

impl<'a, T, const N: usize> ParamSlot<'a, T, N>
where
    T: FromValue + Into<Value> + Copy,
{
    // Crate-visible constructor, called by the macro
    pub fn new(values: &'a ParamStore<N>, index: usize) -> Self {
        assert!(index < N, "StorageSlot index out of bounds");
        Self {
            values,
            index,
            _phantom: PhantomData,
        }
    }

    pub async fn get(&self) -> T {
        let value = self.values.get(self.index).await;
        T::from_value(value)
    }

    pub async fn set(&self, value: T) {
        self.values.set(self.index, value.into()).await
    }

    pub async fn wait_for_change(&self) -> T {
        loop {
            let index = self.values.wait_for_change().await;
            if self.index == index {
                return self.get().await;
            }
        }
    }
}
