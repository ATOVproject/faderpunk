use core::{cell::RefCell, ops::Range};

use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Timer;
use heapless::Vec;
use postcard::{from_bytes, to_slice};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};

use libfp::{
    types::{CalibFile, MaxCalibration, MaxCalibrationV1},
    GlobalConfig, Layout, Value, APP_MAX_PARAMS, CALIB_FILE_MAGIC,
};

use crate::{
    apps::get_channels,
    state::RuntimeState,
    tasks::{
        configure::{AppParamCmd, APP_PARAM_CHANNEL, APP_PARAM_SIGNALS},
        fram::{erase_with, read_data, write_with},
    },
};

const GLOBAL_CONFIG_RANGE: Range<u32> = 0..320;
const RUNTIME_STATE_RANGE: Range<u32> = GLOBAL_CONFIG_RANGE.end..384;
const LAYOUT_RANGE: Range<u32> = RUNTIME_STATE_RANGE.end..512;
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
            if let Ok(mut config) = from_bytes::<GlobalConfig>(data) {
                config.validate();
                return config;
            }
        }
    }
    GlobalConfig::new()
}

pub async fn store_runtime_state(state: &RuntimeState) {
    let res = write_with(RUNTIME_STATE_RANGE.start, |buf| {
        Ok(to_slice(&state, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save runtime state");
    }
}

pub async fn load_runtime_state() -> RuntimeState {
    if let Ok(guard) = read_data(RUNTIME_STATE_RANGE.start).await {
        let data = guard.data();
        if !data.is_empty() {
            if let Ok(state) = from_bytes::<RuntimeState>(data) {
                return state;
            }
        }
    }
    RuntimeState::default()
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
    let layout = Layout::default();
    store_layout(&layout).await;
    layout
}

pub async fn store_calibration_data(data: &MaxCalibration) {
    let file_to_save = CalibFile::new(*data);

    let res = write_with(CALIBRATION_RANGE.start, |buf| {
        Ok(to_slice(&file_to_save, &mut *buf)?.len())
    })
    .await;

    if res.is_err() {
        defmt::error!("Could not save MaxCalibration");
    }
}

pub async fn load_calibration_data() -> Option<MaxCalibration> {
    if let Ok(guard) = read_data(CALIBRATION_RANGE.start).await {
        let data = guard.data();
        if data.len() < 4 {
            // Not enough data to be anything
            return None;
        }

        if data[0..4] == CALIB_FILE_MAGIC {
            if let Ok(file) = from_bytes::<CalibFile>(data) {
                if file.version == 2 {
                    return Some(file.data);
                } else {
                    defmt::warn!("Unsupported calibration file version: {}", file.version);
                    return None;
                }
            }
        } else if let Ok(old_data) = from_bytes::<MaxCalibrationV1>(data) {
            defmt::info!("Old V1 calibration data found, converting to new format.");
            let new_data = MaxCalibration::from(old_data);
            // Re-save the data in the new V2 format for next time
            store_calibration_data(&new_data).await;
            return Some(new_data);
        }

        defmt::warn!("Failed to deserialize calibration data as any known format.");
    }
    None
}

async fn erase_range(range: Range<u32>) {
    // Prevent erasing the calibration range
    if range.start < CALIBRATION_RANGE.end && range.end > CALIBRATION_RANGE.start {
        defmt::error!(
            "CRITICAL: Attempted to erase Protected Calibration Range ({:?}) with request ({:?})",
            CALIBRATION_RANGE,
            range
        );
        return;
    }

    let mut addr = range.start;
    // Limit the chunk size to 64 bytes to prevent I2C bus congestion/timeouts
    const ERASE_CHUNK_SIZE: usize = 64;

    while addr < range.end {
        let mut bytes_written = 0;
        let res = erase_with(addr, |buf| {
            let remaining_bytes = range.end - addr;
            // Use the smaller of: remaining bytes, available buffer, or our safety limit
            let chunk_size = (remaining_bytes as usize)
                .min(buf.len())
                .min(ERASE_CHUNK_SIZE);

            let write_buf = &mut buf[..chunk_size];
            write_buf.fill(0);
            bytes_written = chunk_size;

            Ok(chunk_size)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not erase range starting at {}", addr);
            return;
        }

        if bytes_written == 0 {
            // Avoid infinite loop if write_with provides a zero-length buffer
            defmt::error!("Erase stalled: 0 bytes written at address {}", addr);
            return;
        }
        addr += bytes_written as u32;

        // FRAM writes are instant, just give the bus a tiny breather.
        Timer::after_micros(100).await;
    }
}

/// Erases all data from FRAM except for the calibration data.
pub async fn factory_reset() {
    erase_range(GLOBAL_CONFIG_RANGE).await;
    erase_range(RUNTIME_STATE_RANGE).await;
    erase_range(LAYOUT_RANGE).await;
    erase_range(APP_STORAGE_RANGE).await;
    erase_range(APP_PARAM_RANGE).await;
    // Wait a bit
    Timer::after_millis(100).await;
    // Then restart the unit
    cortex_m::peripheral::SCB::sys_reset();
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
    #[allow(dead_code)]
    pub fn at(&self, idx: usize) -> T {
        self.0[idx]
    }

    #[inline(always)]
    #[allow(dead_code)]
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
    pub layout_id: u8,
    pub scene: Option<u8>,
}

impl From<AppStorageAddress> for u32 {
    fn from(key: AppStorageAddress) -> Self {
        let scene_index = match key.scene {
            None => 0,
            Some(s) => (s as u32) + 1,
        };

        let app_base_offset = (key.layout_id as u32) * (SCENES_PER_APP + 1) * APP_STORAGE_MAX_BYTES;
        let scene_offset_in_app = scene_index * APP_STORAGE_MAX_BYTES;
        APP_STORAGE_RANGE.start + app_base_offset + scene_offset_in_app
    }
}

impl From<u32> for AppStorageAddress {
    fn from(address: u32) -> Self {
        let bytes_per_app_block: u32 = (SCENES_PER_APP + 1) * APP_STORAGE_MAX_BYTES;
        let app_storage_address = address - APP_STORAGE_RANGE.start;

        let layout_id_raw = app_storage_address / bytes_per_app_block;
        let layout_id = layout_id_raw as u8;

        let offset_within_app_block = app_storage_address % bytes_per_app_block;
        let scene_index_raw = offset_within_app_block / APP_STORAGE_MAX_BYTES;

        let scene = if scene_index_raw == 0 {
            None
        } else {
            Some((scene_index_raw - 1) as u8)
        };

        Self { layout_id, scene }
    }
}

impl AppStorageAddress {
    pub fn new(layout_id: u8, scene: Option<u8>) -> Self {
        Self { layout_id, scene }
    }
}

#[derive(Clone, Copy)]
pub struct AppParamsAddress {
    pub layout_id: u8,
}

impl From<AppParamsAddress> for u32 {
    fn from(key: AppParamsAddress) -> Self {
        APP_PARAM_RANGE.start + (key.layout_id as u32) * APP_PARAMS_MAX_BYTES
    }
}

impl From<u32> for AppParamsAddress {
    fn from(address: u32) -> Self {
        let app_storage_address = address - APP_PARAM_RANGE.start;

        let layout_id = (app_storage_address / APP_PARAMS_MAX_BYTES) as u8;

        Self { layout_id }
    }
}

impl AppParamsAddress {
    pub fn new(layout_id: u8) -> Self {
        Self { layout_id }
    }
}

pub trait AppParams: Sized + Default + Send + Sync + 'static {
    fn from_values(values: &[Value]) -> Option<Self>;
    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS>;
}

pub struct ParamStore<P: AppParams> {
    app_id: u8,
    inner: RefCell<P>,
    layout_id: u8,
}

impl<P: AppParams> ParamStore<P> {
    pub fn new(app_id: u8, layout_id: u8) -> Self {
        Self {
            app_id,
            inner: RefCell::new(P::default()),
            layout_id,
        }
    }

    fn des(&self, data: &[u8]) -> Option<P> {
        // First byte is app id
        if data[0] != self.app_id {
            return None;
        }
        if let Ok(val) = from_bytes::<Vec<Value, APP_MAX_PARAMS>>(&data[1..]) {
            return P::from_values(&val);
        }
        None
    }

    async fn send_values(&self) {
        let values = {
            let guard = self.inner.borrow();
            guard.to_values()
        };
        APP_PARAM_CHANNEL.send((self.layout_id, values)).await;
    }

    async fn save(&self) {
        let address = AppParamsAddress::new(self.layout_id);
        let values = {
            let guard = self.inner.borrow_mut();
            guard.to_values()
        };
        let res = write_with(address.into(), |buf| {
            buf[0] = self.app_id;
            let len = to_slice(&values, &mut buf[1..])?.len();
            Ok(len + 1)
        })
        .await;

        if res.is_err() {
            defmt::error!("Could not save ParamStore on app {}", self.app_id);
        }
    }

    pub async fn load(&self) {
        let address = AppParamsAddress::new(self.layout_id);
        if let Ok(guard) = read_data(address.into()).await {
            let data = guard.data();
            if !data.is_empty() {
                if let Some(val) = self.des(data) {
                    drop(guard);
                    let mut inner = self.inner.borrow_mut();
                    *inner = val;
                }
            }
        }
    }

    pub fn query<F, R>(&self, accessor: F) -> R
    where
        F: FnOnce(&P) -> R,
    {
        let guard = self.inner.borrow();
        accessor(&*guard)
    }

    pub async fn param_handler(&self) {
        APP_PARAM_SIGNALS[self.layout_id as usize].reset();
        loop {
            match APP_PARAM_SIGNALS[self.layout_id as usize].wait().await {
                AppParamCmd::SetAppParams { values } => {
                    let mut current_values = self.inner.borrow().to_values();
                    let mut changed = false;

                    for (index, &value) in values.iter().enumerate() {
                        if let Some(val) = value {
                            if index < current_values.len() && current_values[index] != val {
                                current_values[index] = val;
                                changed = true;
                            }
                        }
                    }

                    if changed {
                        let updated = if let Some(new_params) = P::from_values(&current_values) {
                            *self.inner.borrow_mut() = new_params;
                            true
                        } else {
                            false
                        };

                        if updated {
                            self.save().await;
                            self.send_values().await;
                            // Re-spawn app
                            break;
                        }
                    }
                    self.send_values().await;
                }
                AppParamCmd::RequestParamValues => {
                    self.send_values().await;
                }
            }
        }
    }
}

pub trait AppStorage:
    Serialize + for<'de> Deserialize<'de> + Default + Send + Sync + 'static
{
}

pub struct ManagedStorage<S: AppStorage> {
    app_id: u8,
    inner: RefCell<S>,
    layout_id: u8,
    save_signal: Signal<NoopRawMutex, ()>,
}

impl<S: AppStorage> ManagedStorage<S> {
    pub fn new(app_id: u8, layout_id: u8) -> Self {
        Self {
            app_id,
            inner: RefCell::new(S::default()),
            layout_id,
            save_signal: Signal::new(),
        }
    }

    async fn load_inner(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.layout_id, scene).into();
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

    async fn save_inner(&self, scene: Option<u8>) {
        let address = AppStorageAddress::new(self.layout_id, scene).into();

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

    pub async fn save(&self) {
        self.save_inner(None).await;
    }

    pub async fn save_to_scene(&self, scene: u8) {
        self.save_inner(Some(scene)).await;
    }

    pub async fn load(&self) {
        self.load_inner(None).await;
    }

    pub async fn load_from_scene(&self, scene: u8) {
        self.load_inner(Some(scene)).await;
    }

    #[allow(dead_code)]
    pub fn reset(&self) {
        let mut guard = self.inner.borrow_mut();
        *guard = S::default();
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

    pub fn modify_and_save<F, R>(&self, modifier: F) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let result = self.modify(modifier);
        self.save_signal.signal(());
        result
    }

    pub async fn saver_task(&self) {
        loop {
            self.save_signal.wait().await;

            loop {
                let timer = Timer::after_millis(500);
                match select(self.save_signal.wait(), timer).await {
                    // Another signal arrived before the timer finished.
                    // Loop again to restart the timer
                    Either::First(_) => continue,
                    // The timer finished without being interrupted.
                    // Break the inner loop to proceed with saving
                    Either::Second(_) => break,
                }
            }

            self.save_signal.reset();
            self.save_inner(None).await;
        }
    }
}
