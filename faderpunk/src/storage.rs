// FIXME: Clean up this file
use core::marker::PhantomData;

use config::{FromValue, Value};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Serialize};

use crate::tasks::fram::{
    request_data, write_data, FramData, ReadOperation, WriteOperation, MAX_DATA_LEN,
};

const BYTES_PER_VALUE_SET: u32 = 400;
const SCENES_PER_APP: u32 = 3; // Current value + 2 scenes

#[derive(Clone, Copy)]
// TODO: Allocator should alloate a certain part of the fram to app storage
pub struct AppStorageAddress {
    pub start_channel: u8,
    pub scene: Option<u8>,
}

impl From<AppStorageAddress> for u32 {
    fn from(key: AppStorageAddress) -> Self {
        let scene_index = key.scene.unwrap_or(0) as u32;
        let app_base_offset = (key.start_channel as u32) * SCENES_PER_APP * BYTES_PER_VALUE_SET;
        let scene_offset_in_app = scene_index * BYTES_PER_VALUE_SET;
        app_base_offset + scene_offset_in_app
    }
}

impl From<u32> for AppStorageAddress {
    // Changed to take u32
    fn from(address: u32) -> Self {
        let bytes_per_app_block: u32 = SCENES_PER_APP * BYTES_PER_VALUE_SET;

        let start_channel_raw = address / bytes_per_app_block;
        let start_channel = start_channel_raw as u8;

        let offset_within_app_block = address % bytes_per_app_block;
        let scene_index_raw = offset_within_app_block / BYTES_PER_VALUE_SET;

        let scene_index = scene_index_raw as u8;

        let scene = if scene_index == 0 {
            None
        } else {
            Some(scene_index)
        };

        Self {
            start_channel,
            scene,
        }
    }
}

impl AppStorageAddress {
    pub fn new(start_channel: u8, scene: Option<u8>) -> Self {
        Self {
            start_channel,
            scene,
        }
    }
}

pub struct Store<const N: usize> {
    app_id: u8,
    inner: Mutex<NoopRawMutex, [Value; N]>,
    start_channel: u8,
}

impl<const N: usize> Store<N>
where
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub fn new(initial: [Value; N], app_id: u8, start_channel: u8) -> Self {
        Self {
            app_id,
            inner: Mutex::new(initial),
            start_channel,
        }
    }

    async fn ser(&self) -> FramData {
        let data = self.inner.lock().await;
        let mut buf: [u8; MAX_DATA_LEN] = [0; MAX_DATA_LEN];

        // Prepend the app id to the serialized data for easy filtering
        buf[0] = self.app_id;

        // TODO: unwrap
        let len = to_slice(&*data, &mut buf[1..]).unwrap().len();

        Vec::<u8, MAX_DATA_LEN>::from_slice(&buf[..len + 1]).unwrap()
    }

    async fn des(&self, data: &[u8]) -> Option<[Value; N]> {
        // First byte is app id
        if let Ok(val) = from_bytes::<[Value; N]>(&data[1..]) {
            if data[0] != self.app_id {
                return None;
            }
            return Some(val);
        }
        None
    }

    async fn save(&self, scene: Option<u8>) {
        let data = self.ser().await;
        let address = AppStorageAddress::new(self.start_channel, scene);
        let op = WriteOperation::new(address.into(), data);
        write_data(op).await.unwrap();
    }

    pub async fn load(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.start_channel, scene);
        let op = ReadOperation::new(address.into());
        if let Ok(data) = request_data(op).await {
            if data.is_empty() {
                return;
            }
            if let Some(val) = self.des(data.as_slice()).await {
                let mut inner_val = self.inner.lock().await;
                *inner_val = val;
            }
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
        if index >= N {
            return;
        }
        let mut val = self.inner.lock().await;
        val[index] = value;
    }
}

pub struct StorageSlot<'a, T, const N: usize>
where
    T: FromValue + Into<Value> + Copy,
{
    index: usize,
    values: &'a Store<N>,
    _phantom: PhantomData<T>,
}

impl<'a, T, const N: usize> StorageSlot<'a, T, N>
where
    T: FromValue + Into<Value> + Copy,
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub fn new(values: &'a Store<N>, index: usize) -> Self {
        Self {
            index,
            values,
            _phantom: PhantomData,
        }
    }

    pub async fn get(&self) -> T {
        let value = self.values.get(self.index).await;
        T::from_value(value)
    }

    pub async fn set(&self, value: T) {
        self.values.set(self.index, value.into()).await;
    }

    pub async fn save(&self) {
        // TODO: Use a different storage technique for individual storage slots
        // Make sure we can store them individually
        self.values.save(None).await;
    }

    // TODO: This should be on the Store only (as it saves all of it)
    pub async fn save_to_scene(&self, scene: u8) {
        self.values.save(Some(scene)).await;
    }

    pub async fn load(&self) {
        self.values.load(None).await;
    }

    pub async fn load_from_scene(&self, scene: u8) {
        self.values.load(Some(scene)).await;
    }
}

impl<const N: usize> StorageSlot<'_, bool, N>
where
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub async fn toggle(&self) -> bool {
        let value = self.get().await;
        self.set(!value).await;
        !value
    }
}

pub struct ParamSlot<'a, T, const N: usize>
where
    T: FromValue + Into<Value> + Copy,
{
    index: usize,
    signal: Signal<NoopRawMutex, usize>,
    values: &'a Store<N>,
    _phantom: PhantomData<T>,
}

impl<'a, T, const N: usize> ParamSlot<'a, T, N>
where
    T: FromValue + Into<Value> + Copy,
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub fn new(values: &'a Store<N>, index: usize) -> Self {
        assert!(index < N, "ParamSlot index out of bounds");
        Self {
            index,
            signal: Signal::new(),
            values,
            _phantom: PhantomData,
        }
    }

    pub async fn get(&self) -> T {
        let value = self.values.get(self.index).await;
        T::from_value(value)
    }

    pub async fn set(&self, value: T) {
        self.values.set(self.index, value.into()).await;
        self.signal.signal(self.index);
    }

    pub async fn wait_for_change(&self) -> T {
        loop {
            let index = self.signal.wait().await;
            if self.index == index {
                return self.get().await;
            }
        }
    }
}
