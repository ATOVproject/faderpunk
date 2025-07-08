use core::{marker::PhantomData, ops::Range};

use config::{FromValue, GlobalConfig, Value, APP_MAX_PARAMS};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize, Deserializer, Serialize, Serializer,
};

use crate::tasks::{
    configure::{AppParamCmd, APP_PARAM_CHANNEL, APP_PARAM_SIGNALS},
    fram::{read_data, write_with},
};

const GLOBAL_CONFIG_RANGE: Range<u32> = 0..1_024;
const APP_INDEX_RANGE: Range<u32> = GLOBAL_CONFIG_RANGE.end..(GLOBAL_CONFIG_RANGE.end + 1_904);
const APP_STORAGE_RANGE: Range<u32> = APP_INDEX_RANGE.end..122_880;
const APP_PARAM_RANGE: Range<u32> = APP_STORAGE_RANGE.end..131_072;
const APP_STORAGE_MAX_BYTES: u32 = 400;
const APP_PARAMS_MAX_BYTES: u32 = 128;
const SCENES_PER_APP: u32 = 16;

pub async fn store_global_config(config: &GlobalConfig) {
    let res = write_with(GLOBAL_CONFIG_RANGE.start, false, |buf| {
        Ok(to_slice(&config, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save GlobalConfig");
    }
}

pub async fn load_global_config() -> GlobalConfig {
    if let Ok(guard) = read_data(GLOBAL_CONFIG_RANGE.start, None).await {
        let data = guard.data();
        if !data.is_empty() {
            if let Ok(config) = from_bytes::<GlobalConfig>(data) {
                return config;
            }
        }
    }
    GlobalConfig::new()
}

#[derive(Clone, Copy)]
pub struct Arr<T: Sized + Copy + Default, const N: usize>([T; N]);

impl<T: Sized + Copy + Default, const N: usize> Default for Arr<T, N> {
    fn default() -> Self {
        Self([T::default(); N])
    }
}

impl<T: Sized + Copy + Default, const N: usize> Arr<T, N> {
    pub fn new(initial: [T; N]) -> Self {
        Self(initial)
    }

    #[inline(always)]
    pub fn at(&self, idx: usize) -> T {
        self.0[idx]
    }

    #[inline(always)]
    pub fn set_at(&mut self, idx: usize, value: T) {
        self.0[idx] = value;
    }

    #[inline(always)]
    pub fn get(&self) -> [T; N] {
        self.0
    }

    #[inline(always)]
    pub fn set(&mut self, value: [T; N]) {
        self.0 = value;
    }
}

impl<T, const N: usize> Serialize for Arr<T, N>
where
    T: Serialize + Sized + Copy + Default,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec = Vec::<T, N>::from_slice(&self.0).unwrap();
        vec.serialize(serializer)
    }
}

impl<'de, T, const N: usize> Deserialize<'de> for Arr<T, N>
where
    T: Deserialize<'de> + Sized + Copy + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<T, N>::deserialize(deserializer)?;
        if vec.len() != N {
            return Err(D::Error::invalid_length(
                vec.len(),
                &"an array of exact length N",
            ));
        }
        let mut arr = [T::default(); N];
        arr.copy_from_slice(vec.as_slice()); // Safe due to length check above
        Ok(Arr(arr))
    }
}

impl<T: Sized + Copy + PartialEq + Default, const N: usize> PartialEq for Arr<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Clone, Copy)]
// TODO: Allocator should alloate a certain part of the fram to app storage
pub struct AppStorageAddress {
    pub start_channel: usize,
    pub scene: Option<u8>,
}

impl From<AppStorageAddress> for u32 {
    fn from(key: AppStorageAddress) -> Self {
        let scene_index = match key.scene {
            None => 0,
            Some(s) => (s as u32) + 1,
        };

        let app_base_offset =
            (key.start_channel as u32) * (SCENES_PER_APP + 1) * APP_STORAGE_MAX_BYTES;
        let scene_offset_in_app = scene_index * APP_STORAGE_MAX_BYTES;
        APP_STORAGE_RANGE.start + app_base_offset + scene_offset_in_app
    }
}

impl From<u32> for AppStorageAddress {
    fn from(address: u32) -> Self {
        let bytes_per_app_block: u32 = (SCENES_PER_APP + 1) * APP_STORAGE_MAX_BYTES;
        let app_storage_address = address - APP_STORAGE_RANGE.start;

        let start_channel_raw = app_storage_address / bytes_per_app_block;
        let start_channel = start_channel_raw as usize;

        let offset_within_app_block = app_storage_address % bytes_per_app_block;
        let scene_index_raw = offset_within_app_block / APP_STORAGE_MAX_BYTES;

        let scene = if scene_index_raw == 0 {
            None
        } else {
            Some((scene_index_raw - 1) as u8)
        };

        Self {
            start_channel,
            scene,
        }
    }
}

impl AppStorageAddress {
    pub fn new(start_channel: usize, scene: Option<u8>) -> Self {
        Self {
            start_channel,
            scene,
        }
    }
}

#[derive(Clone, Copy)]
pub struct AppParamsAddress {
    pub start_channel: usize,
}

impl From<AppParamsAddress> for u32 {
    fn from(key: AppParamsAddress) -> Self {
        APP_PARAM_RANGE.start + (key.start_channel as u32) * APP_PARAMS_MAX_BYTES
    }
}

impl From<u32> for AppParamsAddress {
    fn from(address: u32) -> Self {
        let app_storage_address = address - APP_PARAM_RANGE.start;

        let start_channel = (app_storage_address / APP_PARAMS_MAX_BYTES) as usize;

        Self { start_channel }
    }
}

impl AppParamsAddress {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }
}

pub struct ParamStore<const N: usize> {
    app_id: u8,
    inner: Mutex<NoopRawMutex, [Value; N]>,
    start_channel: usize,
}

impl<const N: usize> ParamStore<N>
where
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub fn new(initial: [Value; N], app_id: u8, start_channel: usize) -> Self {
        Self {
            app_id,
            inner: Mutex::new(initial),
            start_channel,
        }
    }

    async fn des(&self, data: &[u8]) -> Option<[Value; N]> {
        // First byte is app id
        if data[0] != self.app_id {
            return None;
        }
        if let Ok(val) = from_bytes::<[Value; N]>(&data[1..]) {
            return Some(val);
        }
        None
    }

    async fn save(&self) {
        let address = AppParamsAddress::new(self.start_channel);
        let inner = self.inner.lock().await;
        let res = write_with(address.into(), false, |buf| {
            buf[0] = self.app_id;
            let len = to_slice(&*inner, &mut buf[1..])?.len();
            Ok(len + 1)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not save ParamStore on app {}", self.app_id);
        }
    }

    pub async fn load(&self) {
        let address = AppParamsAddress::new(self.start_channel);
        if let Ok(guard) = read_data(address.into(), None).await {
            let data = guard.data();
            if !data.is_empty() {
                if let Some(val) = self.des(data).await {
                    let mut inner = self.inner.lock().await;
                    *inner = val;
                }
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

    pub async fn param_handler(&self) {
        APP_PARAM_SIGNALS[self.start_channel].reset();
        self.load().await;
        loop {
            match APP_PARAM_SIGNALS[self.start_channel].wait().await {
                AppParamCmd::SetAppParams { values } => {
                    for (index, &value) in values.iter().enumerate() {
                        if let Some(val) = value {
                            self.set(index, val).await;
                        }
                    }
                    self.save().await;
                    // Re-spawn app
                    break;
                }
                AppParamCmd::RequestParamValues => {
                    let params = self.get_all().await;
                    let values: Vec<Value, APP_MAX_PARAMS> = Vec::from_slice(&params).unwrap();
                    APP_PARAM_CHANNEL.send((self.start_channel, values)).await;
                }
            }
        }
    }
}

pub struct ParamSlot<'a, T, const N: usize>
where
    T: FromValue + Into<Value> + Copy,
{
    index: usize,
    values: &'a ParamStore<N>,
    _phantom: PhantomData<T>,
}

impl<'a, T, const N: usize> ParamSlot<'a, T, N>
where
    T: FromValue + Into<Value> + Copy,
    [Value; N]: Serialize,
    [Value; N]: DeserializeOwned,
{
    pub fn new(values: &'a ParamStore<N>, index: usize) -> Self {
        assert!(index < N, "ParamSlot index out of bounds");
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
}

pub trait AppStorage:
    Serialize + for<'de> Deserialize<'de> + Default + Send + Sync + 'static
{
}

pub struct ManagedStorage<S: AppStorage> {
    app_id: u8,
    inner: Mutex<NoopRawMutex, S>,
    start_channel: usize,
}

impl<S: AppStorage> ManagedStorage<S> {
    pub fn new(app_id: u8, start_channel: usize) -> Self {
        Self {
            app_id,
            inner: Mutex::new(S::default()),
            start_channel,
        }
    }

    pub async fn load(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.start_channel, scene).into();
        if let Ok(guard) = read_data(address, None).await {
            let data = guard.data();
            if !data.is_empty() && data[0] == self.app_id {
                if let Ok(val) = from_bytes::<S>(&data[1..]) {
                    let mut inner = self.inner.lock().await;
                    *inner = val;
                }
            }
        }
    }

    pub async fn save(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.start_channel, scene).into();

        let inner = self.inner.lock().await;

        let res = write_with(address, false, |buf| {
            buf[0] = self.app_id;
            let len = to_slice(&*inner, &mut buf[1..])?.len();
            Ok(len + 1)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not save ManagedStorage");
        }
    }

    pub async fn query<F, R>(&self, accessor: F) -> R
    where
        F: FnOnce(&S) -> R,
    {
        let guard = self.inner.lock().await;
        accessor(&*guard)
    }

    pub async fn modify<F, R>(&self, modifier: F) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let mut guard = self.inner.lock().await;
        modifier(&mut *guard)
    }

    pub async fn modify_and_save<F, R>(&self, modifier: F, scene: Option<u8>) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let address = AppStorageAddress::new(self.start_channel, scene).into();

        let mut inner = self.inner.lock().await;
        let result = modifier(&mut *inner);

        let res = write_with(address, false, |buf| {
            buf[0] = self.app_id;
            let len = to_slice(&*inner, &mut buf[1..])?.len();
            Ok(len + 1)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not save ManagedStorage during modify_and_save");
        }

        result
    }
}

#[derive(Debug)]
enum AppDataIndexError {
    WrongSize,
    OutOfBounds,
}

struct AppDataIndex {
    // (app_id, deleted, length, address)
    index: [[(u8, bool, usize, u32); 17]; 16],
    next: u32,
}

impl AppDataIndex {
    // 16 pot. apps * (16 pot. scenes + 1 current state) * (app_id (1) + len (2) + addr (4))
    fn try_new(slice: &[u8]) -> Result<Self, AppDataIndexError> {
        if slice.len() != 1904 {
            return Err(AppDataIndexError::WrongSize);
        }
        let mut entries_iter = slice.chunks_exact(7).map(|chunk| {
            let app_id = chunk[0];

            // These unwraps are safe because chunks_exact guarantees the slice length
            let length_and_deleted = u16::from_le_bytes(chunk[1..3].try_into().unwrap());
            let address = u32::from_le_bytes(chunk[3..7].try_into().unwrap());

            let deleted = (length_and_deleted & 0x8000) != 0;
            let length = (length_and_deleted & 0x7FFF) as usize;

            (app_id, deleted, length, address)
        });

        // This is safe because 1904 / 7 = 272, and 16 * 17 = 272.
        // The iterator will yield the exact number of items needed.
        let index: [[(u8, bool, usize, u32); 17]; 16] =
            core::array::from_fn(|_| core::array::from_fn(|_| entries_iter.next().unwrap()));

        let mut valid_entries: Vec<(u8, bool, usize, u32), 272> = index
            .iter()
            .flatten()
            .filter(|&&(_, _, _, address)| address >= APP_STORAGE_RANGE.start)
            .copied()
            .collect();

        valid_entries.sort_unstable_by_key(|k| k.3);

        let next = if valid_entries.is_empty() {
            APP_STORAGE_RANGE.start
        } else if let Some(last_active_pos) = valid_entries
            .iter()
            .rposition(|&(_, deleted, _, _)| !deleted)
        {
            if last_active_pos == valid_entries.len() - 1 {
                let last_entry = &valid_entries[last_active_pos];
                last_entry.3.saturating_add(last_entry.2 as u32)
            } else {
                valid_entries[last_active_pos + 1].3
            }
        } else {
            // All entries are deleted, reuse the first one's address
            valid_entries[0].3
        };

        Ok(Self { index, next })
    }

    fn serialize_entry(entry: &(u8, bool, usize, u32)) -> [u8; 7] {
        let (app_id, deleted, length, address) = *entry;
        let mut bytes = [0u8; 7];

        // Clamp length to 15 bits to prevent overflow, and set the MSB if deleted.
        let mut length_and_deleted = (length & 0x7FFF) as u16;
        if deleted {
            length_and_deleted |= 0x8000;
        }

        let length_bytes = length_and_deleted.to_le_bytes();
        let address_bytes = address.to_le_bytes();

        bytes[0] = app_id;
        bytes[1..3].copy_from_slice(&length_bytes);
        bytes[3..7].copy_from_slice(&address_bytes);

        bytes
    }

    pub fn to_slice(&self, slice: &mut [u8]) -> Result<(), AppDataIndexError> {
        if slice.len() != 1904 {
            return Err(AppDataIndexError::WrongSize);
        }

        let mut chunks_mut = slice.chunks_exact_mut(7);

        for entry in self.index.iter().flatten() {
            // This unwrap is safe because we checked the total slice length,
            // and the number of entries matches the number of chunks.
            let chunk = chunks_mut.next().unwrap();
            let entry_bytes = Self::serialize_entry(entry);
            chunk.copy_from_slice(&entry_bytes);
        }

        Ok(())
    }

    // TODO: Ideally this should always return _something_
    fn get_next_free(&self, length: usize) -> Result<u32, AppDataIndexError> {
        if self.next + length as u32 > APP_STORAGE_RANGE.end {
            return Err(AppDataIndexError::OutOfBounds);
        }
        Ok(self.next)
    }

    pub async fn allocate_space(
        &mut self,
        app_id: u8,
        channel: usize,
        scene: Option<u8>,
        length: usize,
    ) -> Result<u32, AppDataIndexError> {
        let scene_index = match scene {
            None => 0,
            Some(s) => (s as usize) + 1,
        };

        let data_addr = self.get_next_free(length)?;

        self.index[channel][scene_index] = (app_id, false, length, data_addr);
        self.next = data_addr.saturating_add(length as u32);

        let index_entry_offset = ((channel * 17) + scene_index) * 7;
        let index_entry_addr = APP_INDEX_RANGE.start + index_entry_offset as u32;

        let entry_bytes = Self::serialize_entry(&self.index[channel][scene_index]);

        let res = write_with(index_entry_addr, true, |buf| {
            buf[..7].copy_from_slice(&entry_bytes);
            Ok(7)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not write Index");
        }

        Ok(data_addr)
    }
}
