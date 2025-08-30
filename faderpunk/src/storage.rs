use core::{cell::RefCell, marker::PhantomData, ops::Range};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize, Deserializer, Serialize, Serializer,
};

use libfp::{FromValue, GlobalConfig, Layout, Value, APP_MAX_PARAMS};

use crate::{
    apps::get_channels,
    tasks::{
        configure::{AppParamCmd, APP_PARAM_CHANNEL, APP_PARAM_SIGNALS},
        fram::{read_data, write_with},
        max::MaxCalibration,
    },
};

const GLOBAL_CONFIG_RANGE: Range<u32> = 0..384;
const LAYOUT_RANGE: Range<u32> = GLOBAL_CONFIG_RANGE.end..512;
const CALIBRATION_RANGE: Range<u32> = LAYOUT_RANGE.end..1024;
const APP_STORAGE_RANGE: Range<u32> = CALIBRATION_RANGE.end..122_880;
const APP_PARAM_RANGE: Range<u32> = APP_STORAGE_RANGE.end..131_072;
const APP_STORAGE_MAX_BYTES: u32 = 400;
const APP_PARAMS_MAX_BYTES: u32 = 128;
const SCENES_PER_APP: u32 = 16;

pub async fn store_global_config(config: &GlobalConfig) {
    let res = write_with(GLOBAL_CONFIG_RANGE.start, |buf| {
        Ok(to_slice(&config, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save GlobalConfig");
    }
}

pub async fn load_global_config() -> GlobalConfig {
    if let Ok(guard) = read_data(GLOBAL_CONFIG_RANGE.start).await {
        let data = guard.data();
        if !data.is_empty() {
            if let Ok(config) = from_bytes::<GlobalConfig>(data) {
                return config;
            }
        }
    }
    GlobalConfig::new()
}

pub async fn store_layout(layout: &Layout) {
    let res = write_with(LAYOUT_RANGE.start, |buf| {
        Ok(to_slice(&layout, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save Layout");
    }
}

pub async fn load_layout() -> Layout {
    if let Ok(guard) = read_data(LAYOUT_RANGE.start).await {
        let data = guard.data();
        if !data.is_empty() {
            if let Ok(mut layout) = from_bytes::<Layout>(data) {
                drop(guard);
                // Validate the layout after loading it from fram
                if layout.validate(get_channels) {
                    // If the layout changed after validation, store the validated one
                    store_layout(&layout).await;
                }
                return layout;
            }
        }
    }
    // Fallback layout. We store it directly to start fresh
    let layout = Layout::new();
    store_layout(&layout).await;
    layout
}

pub async fn store_calibration_data(data: &MaxCalibration) {
    let res = write_with(CALIBRATION_RANGE.start, |buf| {
        Ok(to_slice(&data, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save MaxCalibration");
    }
}

pub async fn load_calibration_data() -> Option<MaxCalibration> {
    if let Ok(guard) = read_data(CALIBRATION_RANGE.start).await {
        let data = guard.data();
        if !data.is_empty() {
            if let Ok(calibration_data) = from_bytes::<MaxCalibration>(data) {
                return Some(calibration_data);
            }
        }
    }
    None
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
        let res = write_with(address.into(), |buf| {
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
        if let Ok(guard) = read_data(address.into()).await {
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
    inner: RefCell<S>,
    start_channel: usize,
}

impl<S: AppStorage> ManagedStorage<S> {
    pub fn new(app_id: u8, start_channel: usize) -> Self {
        Self {
            app_id,
            inner: RefCell::new(S::default()),
            start_channel,
        }
    }

    pub async fn load(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.start_channel, scene).into();
        if let Ok(guard) = read_data(address).await {
            let data = guard.data();
            if !data.is_empty() && data[0] == self.app_id {
                if let Ok(val) = from_bytes::<S>(&data[1..]) {
                    let mut inner = self.inner.borrow_mut();
                    *inner = val;
                }
            }
        }
    }

    pub async fn save(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.start_channel, scene).into();

        let res = write_with(address, |buf| {
            buf[0] = self.app_id;
            let inner = self.inner.borrow_mut();
            let len = to_slice(&*inner, &mut buf[1..])?.len();
            Ok(len + 1)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not save ManagedStorage");
        }
    }

    pub fn query<F, R>(&self, accessor: F) -> R
    where
        F: FnOnce(&S) -> R,
    {
        let guard = self.inner.borrow();
        accessor(&*guard)
    }

    pub fn modify<F, R>(&self, modifier: F) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let mut guard = self.inner.borrow_mut();
        modifier(&mut *guard)
    }

    pub async fn modify_and_save<F, R>(&self, modifier: F, scene: Option<u8>) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let address = AppStorageAddress::new(self.start_channel, scene).into();

        let result = {
            let mut inner = self.inner.borrow_mut();
            modifier(&mut *inner)
        };

        let res = write_with(address, |buf| {
            buf[0] = self.app_id;
            let inner = self.inner.borrow();
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
