use core::marker::PhantomData;

use config::{FromValue, Value};
use defmt::info;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::Channel,
    mutex::Mutex,
    pubsub::{PubSubChannel, Publisher},
    signal::Signal,
};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    tasks::eeprom::{AppStorageKey, EepromData, StorageSlotType, DATA_LENGTH},
    CmdSender, HardwareCmd, CMD_CHANNEL,
};

pub const APP_MAX_PARAMS: usize = 8;

pub static APP_STORAGE_CMD_PUBSUB: PubSubChannel<
    CriticalSectionRawMutex,
    AppStorageCmd,
    16,
    16,
    3,
> = PubSubChannel::new();
pub type AppStoragePublisher =
    Publisher<'static, CriticalSectionRawMutex, AppStorageCmd, 16, 16, 3>;

pub static APP_CONFIGURE_EVENT: Channel<CriticalSectionRawMutex, Vec<Value, APP_MAX_PARAMS>, 20> =
    Channel::new();

#[derive(Clone)]
pub enum AppStorageCmd {
    GetAllParams {
        start_channel: u8,
    },
    SetParamSlot {
        start_channel: u8,
        param_slot: u8,
        value: Value,
    },
    ReadAppStorageSlot {
        key: AppStorageKey,
        data: EepromData,
    },
    SaveScene {
        scene: u8,
    },
    LoadScene,
}

// NEXT:
// -> Make own storage channels for storage cmds
// -> Store params in eeprom
// -> Add scenes
// -> Layout changes

pub struct StorageSlot<T: Sized + Copy + Default> {
    app_id: u8,
    cmd_sender: CmdSender,
    inner: Mutex<NoopRawMutex, T>,
    scene_values: Mutex<NoopRawMutex, [Option<T>; 16]>,
    start_channel: u8,
    storage_slot: u8,
}

impl<T: Sized + Copy + Default + PartialEq + Serialize + DeserializeOwned> StorageSlot<T> {
    pub fn new(initial: T, app_id: u8, start_channel: u8, storage_slot: u8) -> Self {
        Self {
            app_id,
            scene_values: Mutex::new([None; 16]),
            inner: Mutex::new(initial),
            start_channel,
            storage_slot,
            cmd_sender: CMD_CHANNEL.sender(),
        }
    }

    async fn ser(&self) -> EepromData {
        let value = self.get().await;
        let mut buf: [u8; DATA_LENGTH] = [0; DATA_LENGTH];

        // Prepend the app id to the serialized data for easy filtering
        buf[0] = self.app_id;

        // TODO: unwrap
        let len = to_slice(&value, &mut buf[1..]).unwrap().len();

        // Store
        Vec::<u8, DATA_LENGTH>::from_slice(&buf[..len + 1]).unwrap()
    }

    async fn des(&self, data: &[u8], scene: Option<u8>) -> Option<T> {
        // First byte is app id
        if let Ok(val) = from_bytes::<T>(&data[1..]) {
            if data[0] != self.app_id {
                return None;
            }
            self.set_impl(val, scene).await;
            return Some(val);
        }
        None
    }

    async fn set_impl(&self, val: T, scene: Option<u8>) {
        if let Some(index) = scene {
            let mut scene_values = self.scene_values.lock().await;
            scene_values[index as usize] = Some(val);
        } else {
            let mut value = self.inner.lock().await;
            *value = val
        }
    }

    async fn save_impl(&self, scene: Option<u8>) {
        let ser = self.ser().await;
        let key = AppStorageKey::new(
            self.start_channel,
            self.storage_slot,
            scene,
            StorageSlotType::Storage,
        );
        self.cmd_sender
            .send(HardwareCmd::EepromStore(key, ser))
            .await;
    }

    pub async fn get(&self) -> T {
        let value = self.inner.lock().await;
        *value
    }

    pub async fn set(&self, val: T) {
        self.set_impl(val, None).await
    }

    pub async fn save(&self) {
        self.save_impl(None).await;
    }

    pub async fn load(&self, key: AppStorageKey, val: EepromData) {
        if key.start_channel == self.start_channel
            && key.storage_slot == self.storage_slot
            && key.slot_type == StorageSlotType::Storage
        {
            self.des(val.as_slice(), key.scene).await;
        }
    }

    pub async fn save_to_scene(&self, scene: u8) {
        let val = self.get().await;
        let mut scene_values = self.scene_values.lock().await;
        if Some(val) == scene_values[scene as usize] {
            // Don't save scene if value is already there
            return;
        }
        scene_values[scene as usize] = Some(self.get().await);
        drop(scene_values);
        info!("SAVED TO SCENE");
        self.save_impl(Some(scene)).await;
    }

    pub async fn load_from_scene(&self, scene: u8) {
        let val = self.get().await;
        let scene_values = self.scene_values.lock().await;
        let value = scene_values[scene as usize];
        if Some(val) == scene_values[scene as usize] {
            // Don't load from scene if value is already there
            return;
        }
        drop(scene_values);
        if let Some(val) = value {
            self.set(val).await;
            self.save().await;
        }
    }
}

impl StorageSlot<bool> {
    pub async fn toggle(&self) -> bool {
        let mut value = self.inner.lock().await;
        *value = !*value;
        *value
    }
}

pub struct Store<const N: usize> {
    app_id: u8,
    inner: Mutex<NoopRawMutex, [Value; N]>,
    // FIXME: Needs to take a storage address instead (for FRAM)
    start_channel: usize,
}

impl<const N: usize> Store<N>
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

    async fn ser(&self) -> EepromData {
        let data = self.inner.lock().await;
        let mut buf: [u8; DATA_LENGTH] = [0; DATA_LENGTH];

        // Prepend the app id to the serialized data for easy filtering
        buf[0] = self.app_id;

        // TODO: unwrap
        let len = to_slice(&*data, &mut buf[1..]).unwrap().len();

        Vec::<u8, DATA_LENGTH>::from_slice(&buf[..len + 1]).unwrap()
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
