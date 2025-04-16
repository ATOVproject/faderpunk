use core::marker::PhantomData;

use config::{FromValue, Param, Value};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    mutex::Mutex,
    signal::Signal,
    watch::Watch,
};
use embassy_time::{with_timeout, Duration};
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Serialize};

use crate::{CmdSender, EventPubSubChannel, HardwareCmd, HardwareEvent, CMD_CHANNEL, EVENT_PUBSUB};

pub const APP_MAX_PARAMS: usize = 8;

// TODO: Find a good number for this (allowed storage size is 64)
pub const DATA_LENGTH: usize = 128;

#[derive(Clone)]
pub enum StorageEvent {
    Read(u8, u8, Vec<u8, DATA_LENGTH>),
}

#[derive(Clone)]
pub enum StorageCmd {
    Request(u8, u8),
    Store(u8, u8, Vec<u8, DATA_LENGTH>),
}

// TODO: PLAN:
// - Create a new low level abstraction to store eeprom stuff.
// - USE FOR StorageSlot only for now!!!

// NEXT:
// -> Make own storage channels for storage cmds
// -> Store params in eeprom
// -> Add scenes
// -> Layout changes

// TODO: For scenes: this has to be adjusted for scenes (we need 4 bits for the scene)
pub fn create_storage_key(app_id: u8, start_channel: u8, storage_slot: u8) -> u32 {
    ((app_id as u32) << 9) | ((storage_slot as u32) << 4) | (start_channel as u32)
}

pub struct StorageSlot<T: Sized + Copy + Default> {
    app_id: usize,
    inner: Mutex<NoopRawMutex, T>,
    start_channel: usize,
    storage_slot: usize,
    cmd_sender: CmdSender,
    event_pubsub: &'static EventPubSubChannel,
}

impl<T: Sized + Copy + Default + Serialize + DeserializeOwned> StorageSlot<T> {
    pub fn new(initial: T, app_id: usize, start_channel: usize, storage_slot: usize) -> Self {
        Self {
            app_id,
            inner: Mutex::new(initial),
            start_channel,
            storage_slot,
            cmd_sender: CMD_CHANNEL.sender(),
            event_pubsub: &EVENT_PUBSUB
        }
    }

    pub fn save_scene(&self) {}

    async fn ser(&self) -> Vec<u8, DATA_LENGTH> {
        let value = self.get().await;
        let mut buf: [u8; DATA_LENGTH] = [0; DATA_LENGTH];
        let serialized = to_slice(&value, &mut buf).unwrap();
        Vec::<u8, DATA_LENGTH>::from_slice(serialized).unwrap()
    }

    async fn des(&self, data: &[u8]) {
        if let Ok(val) = from_bytes::<T>(data) {
            self.set(val).await;
        }
    }

    pub async fn get(&self) -> T {
        let value = self.inner.lock().await;
        *value
    }

    pub async fn set(&self, val: T) {
        let mut value = self.inner.lock().await;
        *value = val
    }

    pub async fn save(&self) {
        let ser = self.ser().await;
        self.cmd_sender
            .send(HardwareCmd::StorageCmd(
                self.start_channel,
                StorageCmd::Store(self.app_id as u8, self.storage_slot as u8, ser),
            ))
            .await;
    }

    pub async fn load(&self) {
        self.cmd_sender
            .send(HardwareCmd::StorageCmd(
                self.start_channel,
                StorageCmd::Request(self.app_id as u8, self.storage_slot as u8),
            ))
            .await;
        // Make this timeout roughly as long as the boot sequence ;)
        with_timeout(Duration::from_millis(2000), async {
            let mut subscriber = self.event_pubsub.subscriber().unwrap();
            loop {
                if let HardwareEvent::StorageEvent(
                    start_channel,
                    StorageEvent::Read(app_id, storage_slot, res),
                ) = subscriber.next_message_pure().await
                {
                    if self.app_id as u8 == app_id
                        && self.storage_slot as u8 == storage_slot
                        && self.start_channel == start_channel
                    {
                        self.des(res.as_slice()).await;
                        return;
                    }
                }
            }
        })
        .await
        .ok();
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
