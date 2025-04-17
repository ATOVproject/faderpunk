use core::marker::PhantomData;

use config::{FromValue, Value};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
    pubsub::{PubSubChannel, Subscriber},
    signal::Signal,
};
use embassy_time::{with_timeout, Duration};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Serialize};

pub const APP_MAX_PARAMS: usize = 8;

// TODO: Find a good number for this (allowed storage size is 64)
pub const DATA_LENGTH: usize = 128;

pub static APP_STORAGE_CMD_PUBSUB: PubSubChannel<
    CriticalSectionRawMutex,
    AppStorageCmd,
    16,
    16,
    2,
> = PubSubChannel::new();
pub static APP_STORAGE_EVENT: Channel<CriticalSectionRawMutex, Vec<Value, APP_MAX_PARAMS>, 20> =
    Channel::new();

// 8 StorageSlots for a maximum of 16 apps. Feels like a lot
pub type StoragePubSub = PubSubChannel<CriticalSectionRawMutex, StorageEvent, 2, { 16 * 8 }, 1>;
pub static STORAGE_EVENT_PUBSUB: StoragePubSub = PubSubChannel::new();
pub type StorageSubscriber =
    Subscriber<'static, CriticalSectionRawMutex, StorageEvent, 2, { 16 * 8 }, 1>;
pub static STORAGE_CMD_CHANNEL: Channel<CriticalSectionRawMutex, StorageCmd, 32> = Channel::new();
pub type CmdSender = Sender<'static, CriticalSectionRawMutex, StorageCmd, 32>;

#[derive(Clone)]
pub enum AppStorageCmd {
    GetAllParams {
        start_channel: usize,
    },
    SetParamSlot {
        start_channel: usize,
        param_slot: usize,
        value: Value,
    },
    SaveScene {
        scene: usize,
    },
    LoadScene {
        scene: usize,
    },
}

#[derive(Clone)]
pub enum StorageEvent {
    Read {
        app_id: u8,
        start_channel: u8,
        storage_slot: u8,
        scene: Option<u8>,
        data: Vec<u8, DATA_LENGTH>,
    },
    NotFound {
        app_id: u8,
        start_channel: u8,
        storage_slot: u8,
        scene: Option<u8>,
    },
}

#[derive(Clone)]
pub enum StorageCmd {
    Request {
        app_id: u8,
        start_channel: u8,
        storage_slot: u8,
        scene: Option<u8>,
    },
    Store {
        app_id: u8,
        start_channel: u8,
        storage_slot: u8,
        scene: Option<u8>,
        data: Vec<u8, DATA_LENGTH>,
    },
}

// TODO: PLAN:
// - Create a new low level abstraction to store eeprom stuff.
// - USE FOR StorageSlot only for now!!!

// NEXT:
// -> Make own storage channels for storage cmds
// -> Store params in eeprom
// -> Add scenes
// -> Layout changes

/// Creates a unique 16‑bit storage key.
///
/// Layout (LSB first):
/// - bits 0‑3 : start_channel (max 16 values)
/// - bits 4‑7 : storage_slot  (max 16 values)
/// - bits 8‑11: scene         (max 16 values) – only relevant if `scene` is `Some(_)`.
/// - bit 12   : is_scene_specific flag (1 = scene‑specific, 0 = global)
/// - bits 13‑15: reserved (0)
pub fn create_storage_key(storage_slot: u8, start_channel: u8, scene: Option<u8>) -> u16 {
    const START_CHANNEL_MASK: u8 = 0b1111; // 4 bits
    const STORAGE_SLOT_MASK: u8 = 0b1111; // 4 bits
    const SCENE_MASK: u8 = 0b1111; // 4 bits

    let masked_start_channel = start_channel & START_CHANNEL_MASK;
    let masked_storage_slot = storage_slot & STORAGE_SLOT_MASK;
    let masked_scene = scene.map_or(0, |sc| sc & SCENE_MASK);
    let scene_flag = scene.is_some() as u16;

    (scene_flag << 12)
        | ((masked_scene as u16) << 8)
        | ((masked_storage_slot as u16) << 4)
        | (masked_start_channel as u16)
}

pub struct StorageSlot<T: Sized + Copy + Default> {
    app_id: usize,
    cmd_sender: CmdSender,
    inner: Mutex<NoopRawMutex, T>,
    scene_values: Mutex<NoopRawMutex, [Option<T>; 16]>,
    start_channel: usize,
    storage_slot: usize,
}

impl<T: Sized + Copy + Default + Serialize + DeserializeOwned> StorageSlot<T> {
    pub fn new(initial: T, app_id: usize, start_channel: usize, storage_slot: usize) -> Self {
        Self {
            app_id,
            scene_values: Mutex::new([None; 16]),
            inner: Mutex::new(initial),
            start_channel,
            storage_slot,
            cmd_sender: STORAGE_CMD_CHANNEL.sender(),
        }
    }

    async fn ser(&self) -> Vec<u8, DATA_LENGTH> {
        let value = self.get().await;
        let mut buf: [u8; DATA_LENGTH] = [0; DATA_LENGTH];
        let serialized = to_slice(&value, &mut buf).unwrap();
        Vec::<u8, DATA_LENGTH>::from_slice(serialized).unwrap()
    }

    async fn des(&self, data: &[u8], scene: Option<u8>) -> Option<T> {
        if let Ok(val) = from_bytes::<T>(data) {
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
        self.cmd_sender
            .send(StorageCmd::Store {
                app_id: self.app_id as u8,
                start_channel: self.start_channel as u8,
                storage_slot: self.storage_slot as u8,
                scene,
                data: ser,
            })
            .await;
    }

    async fn load_impl(&self, scene: Option<u8>) -> Option<T> {
        self.cmd_sender
            .send(StorageCmd::Request {
                app_id: self.app_id as u8,
                start_channel: self.start_channel as u8,
                storage_slot: self.storage_slot as u8,
                scene,
            })
            .await;
        // Make this timeout roughly as long as the boot sequence ;)
        if let Ok(ret) = with_timeout(Duration::from_millis(2000), async {
            let mut subscriber = STORAGE_EVENT_PUBSUB.subscriber().unwrap();
            loop {
                match subscriber.next_message_pure().await {
                    StorageEvent::Read {
                        app_id,
                        start_channel,
                        storage_slot,
                        scene,
                        data,
                    } => {
                        if self.app_id as u8 == app_id
                            && self.storage_slot as u8 == storage_slot
                            && self.start_channel as u8 == start_channel
                        {
                            return self.des(data.as_slice(), scene).await;
                        }
                    }
                    StorageEvent::NotFound {
                        app_id,
                        start_channel,
                        storage_slot,
                        scene: st_scene,
                    } => {
                        if self.app_id as u8 == app_id
                            && self.storage_slot as u8 == storage_slot
                            && self.start_channel as u8 == start_channel
                            && st_scene == scene
                        {
                            return None;
                        }
                    }
                }
            }
        })
        .await
        {
            return ret;
        }
        None
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

    pub async fn load(&self) {
        self.load_impl(None).await;
    }

    pub async fn load_all(&self) {
        for i in 0..16 {
            self.load_impl(Some(i)).await;
        }
        self.load_impl(None).await;
    }

    pub async fn save_to_scene(&self, scene: u8) {
        let mut scene_values = self.scene_values.lock().await;
        scene_values[scene as usize] = Some(self.get().await);
        drop(scene_values);
        self.save_impl(Some(scene)).await;
    }

    pub async fn load_from_scene(&self, scene: u8) {
        let scene_values = self.scene_values.lock().await;
        let value = scene_values[scene as usize];
        drop(scene_values);
        if let Some(val) = value {
            self.set(val).await
        } else if let Some(val) = self.load_impl(Some(scene)).await {
            self.set(val).await;
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

pub struct ParamStore<const N: usize> {
    app_id: usize,
    inner: Mutex<NoopRawMutex, [Value; N]>,
    start_channel: usize,
}

impl<const N: usize> ParamStore<N> {
    pub fn new(initial: [Value; N], app_id: usize, start_channel: usize) -> Self {
        Self {
            app_id,
            inner: Mutex::new(initial),
            start_channel,
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

pub struct ParamSlot<'a, T, const N: usize>
where
    T: FromValue + Into<Value> + Copy,
{
    index: usize,
    signal: Signal<NoopRawMutex, usize>,
    values: &'a ParamStore<N>,
    _phantom: PhantomData<T>,
}

impl<'a, T, const N: usize> ParamSlot<'a, T, N>
where
    T: FromValue + Into<Value> + Copy,
{
    pub fn new(values: &'a ParamStore<N>, index: usize) -> Self {
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
