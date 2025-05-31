use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{with_timeout, Duration};
use fm24v10::Fm24v10;
use heapless::Vec; // For timeouts

// Address is technically a u17
type Address = u32;
type Fram = Fm24v10<'static, I2c<'static, I2C1, Async>>;
pub type FramData = Vec<u8, MAX_DATA_LEN>;
const MAX_CONCURRENT_REQUESTS: usize = 16;
const TIMEOUT_MS: u64 = 200;

const WRITES_CAPACITY: usize = 16;

// TODO: Find a good number for this
pub const MAX_DATA_LEN: usize = 1024;

pub struct WriteOperation {
    address: Address,
    data: FramData,
}

impl WriteOperation {
    pub fn new(address: Address, data: FramData) -> Self {
        Self { address, data }
    }
}

pub struct ReadOperation {
    address: Address,
}

impl ReadOperation {
    pub fn new(address: Address) -> Self {
        Self { address }
    }
}

pub struct Request {
    op: ReadOperation,
    signal_idx: usize,
}

pub static FRAM_WRITE_CHANNEL: Channel<CriticalSectionRawMutex, WriteOperation, WRITES_CAPACITY> =
    Channel::new();

pub type FramReadResult = Result<FramData, FramError>;

pub static FRAM_RESPONSE_SIGNALS_POOL: [Signal<CriticalSectionRawMutex, FramReadResult>;
    MAX_CONCURRENT_REQUESTS] = [const { Signal::new() }; MAX_CONCURRENT_REQUESTS];

pub static FRAM_AVAILABLE_SIGNAL_INDICES: Channel<
    CriticalSectionRawMutex,
    usize,
    MAX_CONCURRENT_REQUESTS,
> = Channel::new();

pub static FRAM_REQUEST_CHANNEL: Channel<
    CriticalSectionRawMutex,
    Request,
    MAX_CONCURRENT_REQUESTS,
> = Channel::new();

#[derive(Debug)]
pub enum FramError {
    /// underlying I²C error
    I2c,
    /// signal index guard error
    SignalIndexGuard,
    /// Timeout in Fram signalling
    Timeout,
    /// Data too big for read buffer
    BufferOverflow,
    /// No data found for address
    Empty,
}

pub struct SignalIndexGuard {
    index: Option<usize>,
}

impl SignalIndexGuard {
    /// Attempts to acquire a signal index from the pool.
    pub async fn acquire() -> Result<Self, FramError> {
        match with_timeout(
            Duration::from_millis(TIMEOUT_MS),
            FRAM_AVAILABLE_SIGNAL_INDICES.receive(), // Wait to receive an index from the channel
        )
        .await
        {
            Ok(idx) => Ok(SignalIndexGuard { index: Some(idx) }),
            Err(_timeout_error) => {
                defmt::warn!("Core 1: Timed out acquiring signal slot.");
                Err(FramError::SignalIndexGuard)
            }
        }
    }

    /// Returns the acquired signal index.
    /// Panics if the index was not acquired (should not happen if `acquire` succeeded).
    pub fn index(&self) -> usize {
        self.index
            .expect("SignalIndexGuard: index not set or already taken. Guard may not have been acquired properly.")
    }
}

impl Drop for SignalIndexGuard {
    fn drop(&mut self) {
        if let Some(idx) = self.index.take() {
            if let Err(_) = FRAM_AVAILABLE_SIGNAL_INDICES.try_send(idx) {
                defmt::error!("CRITICAL: SignalIndexGuard for {} could not be sent", idx);
            }
        }
    }
}

pub async fn request_data(op: ReadOperation) -> Result<FramData, FramError> {
    let guard = SignalIndexGuard::acquire().await?;
    let signal_idx = guard.index();

    FRAM_RESPONSE_SIGNALS_POOL[signal_idx].reset();

    let req = Request { op, signal_idx };
    FRAM_REQUEST_CHANNEL.send(req).await;
    with_timeout(
        Duration::from_millis(TIMEOUT_MS),
        FRAM_RESPONSE_SIGNALS_POOL[signal_idx].wait(),
    )
    .await
    .map_err(|_| FramError::Timeout)
    .and_then(|res| res)
}

pub async fn write_data(op: WriteOperation) -> Result<(), FramError> {
    FRAM_WRITE_CHANNEL.send(op).await;
    Ok(())
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StorageSlotType {
    Param,
    Storage,
}

pub struct Storage {
    /// Fram driver
    fram: Fram,
    /// Write buffer
    write_buf: Vec<u8, { MAX_DATA_LEN + 2 }>,
}

impl Storage {
    pub fn new(fram: Fram) -> Self {
        Self {
            fram,
            write_buf: Vec::new(),
        }
    }

    pub async fn store(&mut self, address: u32, data: &[u8]) -> Result<(), FramError> {
        if data.len() > MAX_DATA_LEN {
            return Err(FramError::BufferOverflow);
        }
        self.write_buf.clear();
        if self.write_buf.resize(2 + data.len(), 0).is_err() {
            return Err(FramError::BufferOverflow);
        }
        let len_bytes = (data.len() as u16).to_le_bytes();
        self.write_buf[0] = len_bytes[0];
        self.write_buf[1] = len_bytes[1];
        self.write_buf[2..2 + data.len()].copy_from_slice(data);
        self.fram
            .write(address, &self.write_buf[..2 + data.len()])
            .await
            .map_err(|_| FramError::I2c)
    }

    pub async fn read(&mut self, address: u32) -> Result<FramData, FramError> {
        let mut len_bytes: [u8; 2] = [0; 2];
        // Read length bytes first
        self.fram
            .read(address, &mut len_bytes)
            .await
            .map_err(|_| FramError::I2c)?;
        let data_length = u16::from_le_bytes(len_bytes) as usize;
        let mut read_buf: FramData = Vec::new();
        if data_length == 0 {
            return Ok(read_buf);
        }
        if read_buf.resize(data_length, 0).is_err() {
            return Err(FramError::BufferOverflow);
        }
        self.fram
            .read(address + 2, &mut read_buf)
            .await
            .map_err(|_| FramError::I2c)?;
        Ok(read_buf)
    }
}

pub async fn start_fram(spawner: &Spawner, fram: Fram) {
    spawner.spawn(run_fram(fram)).unwrap();
}

#[embassy_executor::task]
async fn run_fram(fram: Fram) {
    let write_receiver = FRAM_WRITE_CHANNEL.receiver();
    let read_receiver = FRAM_REQUEST_CHANNEL.receiver();

    // Initialize available signal indices
    for i in 0..MAX_CONCURRENT_REQUESTS {
        if FRAM_AVAILABLE_SIGNAL_INDICES.try_send(i).is_err() {
            panic!(
                "Failed to initialize FRAM_AVAILABLE_SIGNAL_INDICES: channel may be full or capacity is 0. Index: {}",
                i
            );
        }
    }

    // TODO: Add debounced writes (collect write ops and only save at a certain interval)
    // let ticker = Ticker::every(Duration::from_secs(1));

    let mut storage = Storage::new(fram);

    loop {
        match select(read_receiver.receive(), write_receiver.receive()).await {
            Either::First(req) => {
                let result = storage.read(req.op.address).await;
                FRAM_RESPONSE_SIGNALS_POOL[req.signal_idx].signal(result);
            }
            Either::Second(write_op) => {
                storage
                    .store(write_op.address, write_op.data.as_slice())
                    .await
                    .unwrap();
            }
        }
    }
}
