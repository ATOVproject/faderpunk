//! File-backed FRAM: a 128KB in-memory image persisted to disk on every
//! write, standing in for the FM24V10.

use std::path::PathBuf;

use fp_core::tasks::fram::{run_storage_service, StorageBackend};

/// Full capacity of the FM24V10 FRAM the firmware uses.
pub const FRAM_SIZE: usize = 131_072;

struct FileBackend {
    data: Vec<u8>,
    path: PathBuf,
}

impl FileBackend {
    fn new(path: PathBuf) -> Self {
        let mut data = std::fs::read(&path).unwrap_or_default();
        data.resize(FRAM_SIZE, 0);
        Self { data, path }
    }

    fn persist(&self) {
        if let Err(err) = std::fs::write(&self.path, &self.data) {
            log::error!("Failed to persist FRAM image to {:?}: {}", self.path, err);
        }
    }
}

impl StorageBackend for FileBackend {
    async fn read(&mut self, address: u32, buf: &mut [u8]) -> Result<(), ()> {
        let start = address as usize;
        let end = start.checked_add(buf.len()).ok_or(())?;
        if end > self.data.len() {
            return Err(());
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }

    async fn write(&mut self, address: u32, data: &[u8]) -> Result<(), ()> {
        let start = address as usize;
        let end = start.checked_add(data.len()).ok_or(())?;
        if end > self.data.len() {
            return Err(());
        }
        self.data[start..end].copy_from_slice(data);
        self.persist();
        Ok(())
    }
}

#[embassy_executor::task]
pub async fn run_storage(path: PathBuf) {
    log::info!("FRAM image: {}", path.display());
    run_storage_service(FileBackend::new(path)).await
}
