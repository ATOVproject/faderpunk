use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    mutex::{Mutex, MutexGuard},
    signal::Signal,
};
use embassy_time::{with_timeout, Duration};
use fm24v10::Fm24v10;
use heapless::Vec;

// Address is technically a u17
type Address = u32;
type Fram = Fm24v10<'static, I2c<'static, I2C1, Async>>;
pub type FramData = Vec<u8, MAX_DATA_LEN>;
pub type FramReadResult = Result<usize, FramError>;

#[derive(Debug)]
pub enum FramError {
    /// underlying I²C error
    I2c,
    /// signal index guard error
    SignalIndexGuard,
    /// Timeout in Fram signalling
    Timeout,
    /// Data too big for read or write buffer
    BufferOverflow,
    /// No data found for address
    Empty,
}

pub const MAX_DATA_LEN: usize = 1024;

const MAX_CONCURRENT_REQUESTS: usize = 16;
const TIMEOUT_MS: u64 = 200;
const WRITES_CAPACITY: usize = 16;

static WRITE_BUFFER: Mutex<CriticalSectionRawMutex, [u8; MAX_DATA_LEN]> =
    Mutex::new([0; MAX_DATA_LEN]);
static WRITE_BUFFER_TOKEN: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

static READ_BUFFERS: [Mutex<CriticalSectionRawMutex, [u8; MAX_DATA_LEN]>; MAX_CONCURRENT_REQUESTS] =
    [const { Mutex::new([0; MAX_DATA_LEN]) }; MAX_CONCURRENT_REQUESTS];
static AVAILABLE_READ_BUFFER_INDICES: Channel<
    CriticalSectionRawMutex,
    usize,
    MAX_CONCURRENT_REQUESTS,
> = Channel::new();

static RESPONSE_SIGNALS_POOL: [Signal<CriticalSectionRawMutex, FramReadResult>;
    MAX_CONCURRENT_REQUESTS] = [const { Signal::new() }; MAX_CONCURRENT_REQUESTS];

static AVAILABLE_SIGNAL_INDICES: Channel<CriticalSectionRawMutex, usize, MAX_CONCURRENT_REQUESTS> =
    Channel::new();

static WRITE_CHANNEL: Channel<CriticalSectionRawMutex, WriteOperation, WRITES_CAPACITY> =
    Channel::new();

static FRAM_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, Request, MAX_CONCURRENT_REQUESTS> =
    Channel::new();

struct WriteOperation {
    address: Address,
    len: usize,
}

impl WriteOperation {
    fn new(address: Address, len: usize) -> Self {
        Self { address, len }
    }
}

pub struct Request {
    address: Address,
    signal_idx: usize,
    buffer_idx: usize,
}

pub struct ReadGuard {
    len: usize,
    index: usize,
}

impl ReadGuard {
    pub fn len(&self) -> usize {
        self.len
    }

    pub async fn data(&self) -> MutexGuard<'_, CriticalSectionRawMutex, [u8; MAX_DATA_LEN]> {
        READ_BUFFERS[self.index].lock().await
    }
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        // When this guard goes out of scope, the buffer lease is released
        if AVAILABLE_READ_BUFFER_INDICES.try_send(self.index).is_err() {
            defmt::error!("CRITICAL: Failed to release FRAM buffer token. A deadlock will occur.");
        }
    }
}

pub struct SignalIndexGuard {
    index: Option<usize>,
}

impl SignalIndexGuard {
    /// Attempts to acquire a signal index from the pool.
    pub async fn acquire() -> Result<Self, FramError> {
        // Wait to receive an index from the channel
        match with_timeout(
            Duration::from_millis(TIMEOUT_MS),
            AVAILABLE_SIGNAL_INDICES.receive(),
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
            if AVAILABLE_SIGNAL_INDICES.try_send(idx).is_err() {
                defmt::error!("CRITICAL: SignalIndexGuard for {} could not be sent", idx);
            }
        }
    }
}

pub async fn read_data(address: u32) -> Result<ReadGuard, FramError> {
    // Acquire a buffer from the pool with a timeout.
    let buffer_idx = match with_timeout(
        Duration::from_millis(TIMEOUT_MS),
        AVAILABLE_READ_BUFFER_INDICES.receive(),
    )
    .await
    {
        Ok(idx) => idx,
        Err(_) => {
            defmt::warn!("Timed out acquiring a read buffer. System may be overloaded or a ReadGuard was leaked.");
            return Err(FramError::Timeout);
        }
    };
    let guard = SignalIndexGuard::acquire().await?;

    let signal_idx = guard.index();
    RESPONSE_SIGNALS_POOL[signal_idx].reset();

    let req = Request {
        address,
        signal_idx,
        buffer_idx,
    };
    FRAM_REQUEST_CHANNEL.send(req).await;

    // Wait for run_fram to fill the buffer and signal us
    match with_timeout(
        Duration::from_millis(TIMEOUT_MS),
        RESPONSE_SIGNALS_POOL[signal_idx].wait(),
    )
    .await
    {
        Ok(Ok(len)) => Ok(ReadGuard {
            index: buffer_idx,
            len,
        }),
        Ok(Err(e)) => {
            // Must release token on error
            AVAILABLE_READ_BUFFER_INDICES.try_send(buffer_idx).unwrap();
            Err(e)
        }
        Err(_) => {
            // Must release token on timeout
            AVAILABLE_READ_BUFFER_INDICES.try_send(buffer_idx).unwrap();
            Err(FramError::Timeout)
        }
    }
}

pub async fn write_with<F>(address: u32, writer: F) -> Result<(), FramError>
where
    F: FnOnce(&mut [u8]) -> Result<usize, postcard::Error>,
{
    WRITE_BUFFER_TOKEN.receive().await; // Acquire lease on the buffer.

    let len = {
        let mut buffer = WRITE_BUFFER.lock().await;
        // The closure runs here, synchronously, while the lock is held.
        writer(&mut *buffer).map_err(|_| FramError::BufferOverflow)?
    };

    if len > MAX_DATA_LEN {
        WRITE_BUFFER_TOKEN.try_send(()).unwrap(); // Must release token on error
        return Err(FramError::BufferOverflow);
    }

    let op = WriteOperation::new(address, len);
    WRITE_CHANNEL.send(op).await;
    // `run_fram` is now responsible for releasing the token.
    Ok(())
}

struct Storage {
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

    pub async fn read(&mut self, address: u32, data: &mut [u8]) -> Result<usize, FramError> {
        let mut len_bytes: [u8; 2] = [0; 2];
        // Read length bytes first
        self.fram
            .read(address, &mut len_bytes)
            .await
            .map_err(|_| FramError::I2c)?;
        let data_length = u16::from_le_bytes(len_bytes) as usize;
        if data_length == 0 {
            return Ok(0);
        }
        self.fram
            .read(address + 2, data)
            .await
            .map_err(|_| FramError::I2c)?;
        Ok(data_length)
    }
}

pub async fn start_fram(spawner: &Spawner, fram: Fram) {
    spawner.spawn(run_fram(fram)).unwrap();
}

#[embassy_executor::task]
async fn run_fram(fram: Fram) {
    let write_receiver = WRITE_CHANNEL.receiver();
    let read_receiver = FRAM_REQUEST_CHANNEL.receiver();

    for i in 0..MAX_CONCURRENT_REQUESTS {
        AVAILABLE_SIGNAL_INDICES.try_send(i).unwrap();
    }
    for i in 0..MAX_CONCURRENT_REQUESTS {
        AVAILABLE_READ_BUFFER_INDICES.try_send(i).unwrap();
    }

    WRITE_BUFFER_TOKEN
        .try_send(())
        .expect("Failed to initialize BUFFER_TOKEN");

    // TODO: Add debounced writes (collect write ops and only save at a certain interval)
    // let ticker = Ticker::every(Duration::from_secs(1));

    let mut storage = Storage::new(fram);

    loop {
        match select(read_receiver.receive(), write_receiver.receive()).await {
            Either::First(req) => {
                let mut data = READ_BUFFERS[req.buffer_idx].lock().await;
                let result = storage.read(req.address, &mut *data).await;
                // Signal the caller. The caller's ReadGuard is responsible for releasing the lease
                // as we need to know when the caller is "done" with the data
                RESPONSE_SIGNALS_POOL[req.signal_idx].signal(result);
            }
            Either::Second(write_op) => {
                let data = WRITE_BUFFER.lock().await;
                storage
                    .store(write_op.address, &data[..write_op.len])
                    .await
                    .unwrap();
                drop(data);
                // Release the lease, making the buffer available for the next operation
                WRITE_BUFFER_TOKEN.send(()).await;
            }
        }
    }
}
