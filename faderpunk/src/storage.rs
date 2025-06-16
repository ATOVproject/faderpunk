use core::{marker::PhantomData, ops::Range};

use config::{FromValue, GlobalConfig, Value, APP_MAX_PARAMS};
use defmt::Debug2Format;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Serialize};

use crate::tasks::{
    configure::{AppParamCmd, APP_PARAM_CHANNEL, APP_PARAM_SIGNALS},
    fram::{request_data, write_data, ReadOperation, WriteOperation, FRAM_WRITE_BUF, MAX_DATA_LEN},
};

const GLOBAL_CONFIG_RANGE: Range<u32> = 0..1_024;
const APP_STORAGE_RANGE: Range<u32> = GLOBAL_CONFIG_RANGE.end..122_880;
const APP_PARAM_RANGE: Range<u32> = APP_STORAGE_RANGE.end..131_072;
const APP_STORAGE_MAX_BYTES: u32 = 400;
const APP_PARAMS_MAX_BYTES: u32 = 128;
const SCENES_PER_APP: u32 = 3;

pub async fn store_global_config(config: &GlobalConfig) {
    let mut buf = FRAM_WRITE_BUF.lock().await;
    let len = to_slice(&config, &mut *buf).unwrap().len();
    drop(buf);
    match WriteOperation::try_new(GLOBAL_CONFIG_RANGE.start, len) {
        Ok(op) => {
            write_data(op).await.unwrap();
        }
        Err(_) => defmt::error!("Could not write GlobalConfig"),
    }
}

pub async fn load_global_config() -> GlobalConfig {
    let address = GLOBAL_CONFIG_RANGE.start;
    let op = ReadOperation::new(address);
    match request_data(op).await {
        Ok(data) => match from_bytes::<GlobalConfig>(&data) {
            Ok(global_config) => {
                return global_config;
            }
            Err(err) => {
                defmt::error!("Could not parse GlobalConfig: {:?}", Debug2Format(&err));
            }
        },
        Err(err) => {
            defmt::error!("Could not read GlobalConfig: {:?}", Debug2Format(&err));
        }
    }
    GlobalConfig::default()
    // request_data(op)
    //     .await
    //     .ok()
    //     .and_then(|data| from_bytes::<GlobalConfig>(&data).ok())
    //     .unwrap_or_else(GlobalConfig::default)
}

#[derive(Clone, Copy)]
// TODO: Allocator should alloate a certain part of the fram to app storage
pub struct AppStorageAddress {
    pub start_channel: usize,
    pub scene: Option<u8>,
}

impl From<AppStorageAddress> for u32 {
    fn from(key: AppStorageAddress) -> Self {
        let scene_index = key.scene.unwrap_or(0) as u32;
        let app_base_offset = (key.start_channel as u32) * SCENES_PER_APP * APP_STORAGE_MAX_BYTES;
        let scene_offset_in_app = scene_index * APP_STORAGE_MAX_BYTES;
        APP_STORAGE_RANGE.start + app_base_offset + scene_offset_in_app
    }
}

impl From<u32> for AppStorageAddress {
    fn from(address: u32) -> Self {
        let bytes_per_app_block: u32 = SCENES_PER_APP * APP_STORAGE_MAX_BYTES;
        let app_storage_address = address - APP_STORAGE_RANGE.start;

        let start_channel_raw = app_storage_address / bytes_per_app_block;
        let start_channel = start_channel_raw as usize;

        let offset_within_app_block = app_storage_address % bytes_per_app_block;
        let scene_index_raw = offset_within_app_block / APP_STORAGE_MAX_BYTES;

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

    async fn ser(&self, buf: &mut [u8]) -> usize {
        let data = self.inner.lock().await;
        // Prepend the app id to the serialized data for easy filtering
        buf[0] = self.app_id;
        // TODO: unwrap
        to_slice(&*data, &mut buf[1..]).unwrap().len()
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

    async fn save(&self) {
        let mut buf = FRAM_WRITE_BUF.lock().await;
        let len = self.ser(&mut *buf).await;
        let address = AppParamsAddress::new(self.start_channel);
        if let Ok(op) = WriteOperation::try_new(address.into(), len) {
            write_data(op).await.unwrap();
        }
    }

    pub async fn load(&self) {
        let address = AppParamsAddress::new(self.start_channel);
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
