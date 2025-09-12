use core::{future::pending, mem::MaybeUninit, slice::from_raw_parts};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex, signal::Signal,
};
use embassy_time::{with_timeout, Duration};
use fm24v10::Fm24v10;
use heapless::Vec;
use libfp::Color;

use crate::{
    app::Led,
    tasks::leds::{set_led_overlay_mode, LedMode},
};

// Address is technically a u17
type Address = u32;
type Fram = Fm24v10<'static, I2c<'static, I2C1, Async>>;
/// The result of a read operation inside the driver task.
type FramReadResult = Result<usize, FramError>;

#[derive(Debug, defmt::Format, PartialEq, Eq, Clone, Copy)]
pub enum FramError {
    /// underlying I²C error
    I2c,
    /// signal index guard error
    SignalIndexGuard,
    /// Timeout in Fram signalling or buffer acquisition
    Timeout,
    /// Data too big for read or write buffer
    BufferOverflow,
    /// No data found for address (read length was 0)
    Empty,
    /// CRC check failed
    CrcMismatch,
}

pub const MAX_DATA_LEN: usize = 384;
const MAX_CONCURRENT_REQUESTS: usize = 16;
const TIMEOUT_MS: u64 = 200;
const WRITES_CAPACITY: usize = 16;

static WRITE_BUFFER: Mutex<CriticalSectionRawMutex, [u8; MAX_DATA_LEN]> =
    Mutex::new([0; MAX_DATA_LEN]);
static WRITE_BUFFER_TOKEN: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();
static WRITE_CHANNEL: Channel<CriticalSectionRawMutex, WriteOperation, WRITES_CAPACITY> =
    Channel::new();

static mut READ_BUFFERS: [MaybeUninit<[u8; MAX_DATA_LEN]>; MAX_CONCURRENT_REQUESTS] =
    [MaybeUninit::uninit(); MAX_CONCURRENT_REQUESTS];

static AVAILABLE_READ_BUFFER_INDICES: Channel<
    CriticalSectionRawMutex,
    usize,
    MAX_CONCURRENT_REQUESTS,
> = Channel::new();
static FRAM_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, Request, MAX_CONCURRENT_REQUESTS> =
    Channel::new();

static RESPONSE_SIGNALS_POOL: [Signal<CriticalSectionRawMutex, FramReadResult>;
    MAX_CONCURRENT_REQUESTS] = [const { Signal::new() }; MAX_CONCURRENT_REQUESTS];
static AVAILABLE_SIGNAL_INDICES: Channel<CriticalSectionRawMutex, usize, MAX_CONCURRENT_REQUESTS> =
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

/// A guard that provides access to the read data.
/// Its existence proves exclusive access to one of the static read buffers.
/// When it is dropped, the buffer is automatically returned to the pool.
pub struct ReadGuard {
    len: usize,
    index: usize,
}

impl ReadGuard {
    /// Returns a slice to the data read from FRAM, using direct pointer acceess
    pub fn data(&self) -> &[u8] {
        // SAFETY: The existence of this ReadGuard proves we hold the "lease"
        // on this buffer index, obtained from `AVAILABLE_READ_BUFFER_INDICES`.
        // No other task can be accessing this buffer. The slice is bounded by `len`,
        // which is the actual amount of data read by the driver.
        unsafe {
            let buffer_ptr = READ_BUFFERS[self.index].as_ptr();
            from_raw_parts(buffer_ptr as *const u8, self.len)
        }
    }
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        // Return the buffer "lease" to the pool so it can be reused.
        if AVAILABLE_READ_BUFFER_INDICES.try_send(self.index).is_err() {
            defmt::error!("CRITICAL: Failed to release FRAM buffer lease. A buffer is now lost and a deadlock may occur.");
        }
    }
}

/// A guard to ensure a signal from the pool is always returned.
pub struct SignalIndexGuard {
    index: Option<usize>,
}

impl SignalIndexGuard {
    pub async fn acquire() -> Result<Self, FramError> {
        match with_timeout(
            Duration::from_millis(TIMEOUT_MS),
            AVAILABLE_SIGNAL_INDICES.receive(),
        )
        .await
        {
            Ok(idx) => Ok(SignalIndexGuard { index: Some(idx) }),
            Err(_timeout_error) => Err(FramError::SignalIndexGuard),
        }
    }

    pub fn index(&self) -> usize {
        self.index
            .expect("SignalIndexGuard used after being consumed")
    }
}

impl Drop for SignalIndexGuard {
    fn drop(&mut self) {
        if let Some(idx) = self.index.take() {
            if AVAILABLE_SIGNAL_INDICES.try_send(idx).is_err() {
                defmt::error!(
                    "CRITICAL: SignalIndexGuard for {} could not be sent back to pool.",
                    idx
                );
            }
        }
    }
}

pub async fn read_data(address: u32) -> Result<ReadGuard, FramError> {
    let buffer_idx = match with_timeout(
        Duration::from_millis(TIMEOUT_MS),
        AVAILABLE_READ_BUFFER_INDICES.receive(),
    )
    .await
    {
        Ok(idx) => idx,
        Err(_) => {
            defmt::warn!("Timed out acquiring a read buffer lease.");
            return Err(FramError::Timeout);
        }
    };

    let signal_guard = SignalIndexGuard::acquire().await?;
    let signal_idx = signal_guard.index();
    RESPONSE_SIGNALS_POOL[signal_idx].reset();

    let req = Request {
        address,
        signal_idx,
        buffer_idx,
    };
    FRAM_REQUEST_CHANNEL.send(req).await;

    match with_timeout(
        Duration::from_millis(TIMEOUT_MS),
        RESPONSE_SIGNALS_POOL[signal_idx].wait(),
    )
    .await
    {
        Ok(Ok(len)) => {
            if len > 0 {
                Ok(ReadGuard {
                    index: buffer_idx,
                    len,
                })
            } else {
                // If 0 bytes were read, it's an empty record. Release the buffer immediately
                AVAILABLE_READ_BUFFER_INDICES.try_send(buffer_idx).unwrap();
                Err(FramError::Empty)
            }
        }
        // Release buffer lease!
        Ok(Err(e)) => {
            AVAILABLE_READ_BUFFER_INDICES.try_send(buffer_idx).unwrap();
            Err(e)
        }
        // Release buffer lease!
        Err(_) => {
            AVAILABLE_READ_BUFFER_INDICES.try_send(buffer_idx).unwrap();
            Err(FramError::Timeout)
        }
    }
}

pub async fn write_with<F>(address: u32, writer: F) -> Result<(), FramError>
where
    F: FnOnce(&mut [u8]) -> Result<usize, postcard::Error>,
{
    WRITE_BUFFER_TOKEN.receive().await;

    let len = {
        let mut buffer = WRITE_BUFFER.lock().await;
        writer(&mut *buffer).map_err(|_| FramError::BufferOverflow)?
    };

    if len > MAX_DATA_LEN {
        WRITE_BUFFER_TOKEN.try_send(()).unwrap();
        return Err(FramError::BufferOverflow);
    }

    let op = WriteOperation::new(address, len);
    WRITE_CHANNEL.send(op).await;
    // `run_fram` is now responsible for releasing the token.
    Ok(())
}

fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0, |acc, &byte| acc.wrapping_add(byte))
}

struct Storage {
    fram: Fram,
    write_buf: Vec<u8, { MAX_DATA_LEN + 2 }>,
}

impl Storage {
    pub fn new(fram: Fram) -> Self {
        Self {
            fram,
            write_buf: Vec::new(),
        }
    }

    /// Writes data to FRAM, prefixing it with a 2-byte length and a 1-byte checksum.
    pub async fn store(&mut self, address: u32, data: &[u8]) -> Result<(), FramError> {
        if data.len() > MAX_DATA_LEN {
            return Err(FramError::BufferOverflow);
        }
        self.write_buf.clear();

        let checksum = calculate_checksum(data);

        // Resize buffer to hold header (2 bytes length + 1 byte checksum) + data
        if self.write_buf.resize(3 + data.len(), 0).is_err() {
            return Err(FramError::BufferOverflow);
        }
        // Write the header and then the data
        let len_bytes = (data.len() as u16).to_le_bytes();
        self.write_buf[0] = len_bytes[0];
        self.write_buf[1] = len_bytes[1];
        self.write_buf[2] = checksum;
        self.write_buf[3..].copy_from_slice(data);

        self.fram
            .write(address, &self.write_buf)
            .await
            .map_err(|_| FramError::I2c)
    }

    /// Reads data from FRAM. It attempts to verify a checksum and will automatically
    /// migrate data from an old format (without checksum) to the new format.
    /// WARNING: A genuine checksum failure on new-format data will be misinterpreted
    /// as old-format data, which may lead to silent data corruption.
    pub async fn read(&mut self, address: u32, data_buf: &mut [u8]) -> FramReadResult {
        const HEADER_SIZE_NEW: u32 = 3;
        const HEADER_SIZE_OLD: u32 = 2;

        let mut len_bytes: [u8; 2] = [0; 2];
        self.fram
            .read(address, &mut len_bytes)
            .await
            .map_err(|_| FramError::I2c)?;
        let data_length = u16::from_le_bytes(len_bytes) as usize;

        if data_length == 0 {
            return Ok(0);
        }
        if data_length > data_buf.len() {
            return Err(FramError::BufferOverflow);
        }

        // Try reading as new format with checksum
        let mut stored_checksum_byte: [u8; 1] = [0; 1];
        self.fram
            .read(address + HEADER_SIZE_OLD, &mut stored_checksum_byte)
            .await
            .map_err(|_| FramError::I2c)?;
        let stored_checksum = stored_checksum_byte[0];

        let read_slice = &mut data_buf[..data_length];
        self.fram
            .read(address + HEADER_SIZE_NEW, read_slice)
            .await
            .map_err(|_| FramError::I2c)?;

        let calculated_checksum = calculate_checksum(read_slice);

        if stored_checksum == calculated_checksum {
            // Checksum is valid, data is in new format.
            Ok(data_length)
        } else {
            // Checksum mismatch. Assume old format and migrate.
            defmt::warn!(
                "CRC mismatch at addr {}. Assuming old data format, migrating...",
                address
            );

            // Re-read data from the old offset.
            self.fram
                .read(address + HEADER_SIZE_OLD, read_slice)
                .await
                .map_err(|_| FramError::I2c)?;

            // Write the data back in the new format to migrate it.
            self.store(address, read_slice).await?;

            Ok(data_length)
        }
    }

    // TODO: Uncomment in next version once everyone had their data migrated
    // /// Reads length-prefixed and checksummed data from FRAM into the provided buffer.
    // pub async fn read(&mut self, address: u32, data_buf: &mut [u8]) -> FramReadResult {
    //     const HEADER_SIZE: u32 = 3;
    //     let mut header: [u8; HEADER_SIZE as usize] = [0; HEADER_SIZE as usize];
    //
    //     // Read the 3-byte header first.
    //     self.fram
    //         .read(address, &mut header)
    //         .await
    //         .map_err(|_| FramError::I2c)?;
    //     let len_bytes = [header[0], header[1]];
    //     let stored_checksum = header[2];
    //     let data_length = u16::from_le_bytes(len_bytes) as usize;
    //
    //     if data_length == 0 {
    //         // No data to read.
    //         return Ok(0);
    //     }
    //     if data_length > data_buf.len() {
    //         return Err(FramError::BufferOverflow);
    //     }
    //
    //     // Read the actual data into the provided buffer.
    //     let read_slice = &mut data_buf[..data_length];
    //     self.fram
    //         .read(address + HEADER_SIZE, read_slice)
    //         .await
    //         .map_err(|_| FramError::I2c)?;
    //
    //     let calculated_checksum = calculate_checksum(read_slice);
    //     if stored_checksum != calculated_checksum {
    //         defmt::error!(
    //             "FRAM checksum mismatch! Stored: {}, calculated: {}",
    //             stored_checksum,
    //             calculated_checksum
    //         );
    //         return Err(FramError::CrcMismatch);
    //     }
    //
    //     Ok(data_length)
    // }
}

pub async fn start_fram(spawner: &Spawner, mut fram: Fram) {
    // Initialization check. If fram can't be read from, stall and flash LED
    let mut len_bytes: [u8; 2] = [0; 2];
    if fram.read(0, &mut len_bytes).await.is_err() {
        set_led_overlay_mode(0, Led::Button, LedMode::Flash(Color::Red, None)).await;
        pending::<()>().await;
    }

    spawner.spawn(run_fram(fram)).unwrap();
}

#[embassy_executor::task]
async fn run_fram(fram: Fram) {
    for i in 0..MAX_CONCURRENT_REQUESTS {
        AVAILABLE_SIGNAL_INDICES
            .try_send(i)
            .expect("Failed to get an available signal index");
        AVAILABLE_READ_BUFFER_INDICES
            .try_send(i)
            .expect("Failed to get an available read buffer");
    }
    WRITE_BUFFER_TOKEN
        .try_send(())
        .expect("Failed to initialize WRITE_BUFFER_TOKEN");

    let mut storage = Storage::new(fram);

    loop {
        match select(FRAM_REQUEST_CHANNEL.receive(), WRITE_CHANNEL.receive()).await {
            // A read was requested
            Either::First(req) => {
                // SAFETY: The `req.buffer_idx` is guaranteed to be
                // uniquely "owned" by this flow until the caller's ReadGuard is dropped.
                // The caller is asleep until we signal a result
                let buffer = unsafe { READ_BUFFERS[req.buffer_idx].assume_init_mut() };

                let result = storage.read(req.address, buffer).await;

                // Signal the caller with the result (Ok(len) or Err).
                // The caller is now responsible for the buffer lease via its ReadGuard.
                RESPONSE_SIGNALS_POOL[req.signal_idx].signal(result);
            }
            // A write was requested
            Either::Second(write_op) => {
                let data_guard = WRITE_BUFFER.lock().await;

                if let Err(e) = storage
                    .store(write_op.address, &data_guard[..write_op.len])
                    .await
                {
                    defmt::error!("FRAM store failed: {:?}", e);
                }

                drop(data_guard);

                // Release the lease, making the buffer available for the next write operation.
                WRITE_BUFFER_TOKEN.send(()).await;
            }
        }
    }
}
