//! Transactional fixed-slot storage for installable applications.

use crate::fpapp::{crc32, Package, PackageError, Version};

pub const FPAPP_REGION_SIZE: usize = 512 * 1024;
pub const SLOT_COUNT: usize = 4;
pub const SLOT_SIZE: usize = FPAPP_REGION_SIZE / SLOT_COUNT;
pub const ERASE_SIZE: usize = 4096;
/// How long a half-finished upload keeps its slot reserved. Staging lives only
/// in RAM, so an uploader that goes away mid-transfer (browser closed, cable
/// pulled) would otherwise hold `Busy` until the next power cycle, blocking
/// both installs and removals with no way out from the UI.
pub const STAGING_TIMEOUT_MS: u64 = 30_000;
pub const PACKAGE_OFFSET: usize = ERASE_SIZE;
pub const MAX_PACKAGE_SIZE: usize = SLOT_SIZE - PACKAGE_OFFSET;

const CONTROL_MAGIC: [u8; 8] = *b"FPSLOT\r\n";
const CONTROL_VERSION: u16 = 0;
const CONTROL_BODY_LEN: usize = 26;
const CONTROL_RECORD_LEN: usize = CONTROL_BODY_LEN + 4;

pub trait SlotFlash {
    type Error;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn mapped(&self, offset: usize, len: usize) -> Result<&[u8], Self::Error>;
    fn erase(&mut self, offset: usize, len: usize) -> Result<(), Self::Error>;
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum StoreError<E> {
    Flash(E),
    RegionTooSmall,
    InvalidSlot,
    Busy,
    NoInstall,
    EmptyPackage,
    PackageTooLarge,
    UnexpectedOffset { expected: usize },
    ChunkTooLarge,
    Incomplete,
    ActiveApp,
    IncompatibleFirmware,
    DuplicateAppId,
    Package(PackageError),
}

impl<E> From<PackageError> for StoreError<E> {
    fn from(error: PackageError) -> Self {
        Self::Package(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub slot: u8,
    pub app_id: u8,
    pub version: Version,
    pub package_len: u32,
}

#[derive(Clone, Copy)]
struct SlotEntry {
    app_id: u8,
    version: Version,
    package_len: u32,
    package_crc: u32,
}

#[derive(Clone, Copy)]
struct Staging {
    slot: usize,
    total_len: usize,
    received: usize,
    /// Caller-supplied clock reading from the last `begin_install`/`write_chunk`.
    last_activity_ms: u64,
}

pub struct SlotStore<F: SlotFlash> {
    flash: F,
    firmware_abi: [u8; 32],
    entries: [Option<SlotEntry>; SLOT_COUNT],
    staging: Option<Staging>,
}

impl<F: SlotFlash> SlotStore<F> {
    pub fn open(flash: F, firmware_abi: [u8; 32]) -> Result<Self, StoreError<F::Error>> {
        if flash.len() < FPAPP_REGION_SIZE {
            return Err(StoreError::RegionTooSmall);
        }

        let mut entries = [None; SLOT_COUNT];
        let mut app_ids = [0u8; SLOT_COUNT];
        let mut app_count = 0;
        for (slot, destination) in entries.iter_mut().enumerate() {
            let Some(entry) = read_slot(&flash, slot, &firmware_abi).map_err(StoreError::Flash)?
            else {
                continue;
            };
            if app_ids[..app_count].contains(&entry.app_id) {
                continue;
            }
            app_ids[app_count] = entry.app_id;
            app_count += 1;
            *destination = Some(entry);
        }

        Ok(Self {
            flash,
            firmware_abi,
            entries,
            staging: None,
        })
    }

    pub fn begin_install(
        &mut self,
        slot: usize,
        total_len: usize,
        active_app_ids: &[u8],
        now_ms: u64,
    ) -> Result<(), StoreError<F::Error>> {
        if let Some(staging) = self.staging {
            if now_ms.saturating_sub(staging.last_activity_ms) < STAGING_TIMEOUT_MS {
                return Err(StoreError::Busy);
            }
            // The previous uploader stopped talking to us. Its bytes were never
            // committed, so dropping the reservation loses nothing and lets the
            // user retry instead of power-cycling.
            self.staging = None;
        }
        if slot >= SLOT_COUNT {
            return Err(StoreError::InvalidSlot);
        }
        if total_len == 0 {
            return Err(StoreError::EmptyPackage);
        }
        if total_len > MAX_PACKAGE_SIZE {
            return Err(StoreError::PackageTooLarge);
        }
        if self.entries[slot].is_some_and(|entry| active_app_ids.contains(&entry.app_id)) {
            return Err(StoreError::ActiveApp);
        }

        let base = slot_offset(slot);
        // Invalidate first. A reset from this point through commit exposes an
        // empty slot, which the user can recover by re-uploading.
        self.flash
            .erase(base, ERASE_SIZE)
            .map_err(StoreError::Flash)?;
        self.entries[slot] = None;
        let erase_len = total_len
            .checked_add(ERASE_SIZE - 1)
            .ok_or(StoreError::PackageTooLarge)?
            & !(ERASE_SIZE - 1);
        self.flash
            .erase(base + PACKAGE_OFFSET, erase_len)
            .map_err(StoreError::Flash)?;
        self.staging = Some(Staging {
            slot,
            total_len,
            received: 0,
            last_activity_ms: now_ms,
        });
        Ok(())
    }

    pub fn write_chunk(
        &mut self,
        offset: usize,
        data: &[u8],
        now_ms: u64,
    ) -> Result<(), StoreError<F::Error>> {
        let staging = self.staging.as_mut().ok_or(StoreError::NoInstall)?;
        if offset != staging.received {
            return Err(StoreError::UnexpectedOffset {
                expected: staging.received,
            });
        }
        let end = offset
            .checked_add(data.len())
            .ok_or(StoreError::ChunkTooLarge)?;
        if end > staging.total_len {
            return Err(StoreError::ChunkTooLarge);
        }
        self.flash
            .write(package_offset(staging.slot) + offset, data)
            .map_err(StoreError::Flash)?;
        staging.received = end;
        staging.last_activity_ms = now_ms;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<InstalledApp, StoreError<F::Error>> {
        let staging = self.staging.ok_or(StoreError::NoInstall)?;
        let (app_id, version, package_crc) = {
            let package = self.staged_package()?;
            let package_bytes = self
                .flash
                .mapped(package_offset(staging.slot), staging.total_len)
                .map_err(StoreError::Flash)?;
            (
                package.manifest.app_id,
                package.manifest.version,
                crc32(package_bytes),
            )
        };

        let installed = InstalledApp {
            slot: staging.slot as u8,
            app_id,
            version,
            package_len: staging.total_len as u32,
        };
        let entry = SlotEntry {
            app_id: installed.app_id,
            version: installed.version,
            package_len: installed.package_len,
            package_crc,
        };

        let control = encode_control(staging.slot, entry);
        self.flash
            .write(slot_offset(staging.slot), &control)
            .map_err(StoreError::Flash)?;

        self.entries[staging.slot] = Some(entry);
        self.staging = None;
        Ok(installed)
    }

    /// Return the fully validated staging package without publishing its slot.
    /// Firmware uses this seam for target-specific runtime checks which cannot
    /// live in the allocation-free, hardware-independent store.
    pub fn staged_package(&self) -> Result<Package<'_>, StoreError<F::Error>> {
        let staging = self.staging.ok_or(StoreError::NoInstall)?;
        if staging.received != staging.total_len {
            return Err(StoreError::Incomplete);
        }

        let package_bytes = self
            .flash
            .mapped(package_offset(staging.slot), staging.total_len)
            .map_err(StoreError::Flash)?;
        let package = Package::parse(package_bytes)?;
        package.native_program()?;
        if package.manifest.firmware_abi != self.firmware_abi {
            return Err(StoreError::IncompatibleFirmware);
        }
        if self.entries.iter().enumerate().any(|(slot, entry)| {
            slot != staging.slot
                && entry.is_some_and(|entry| entry.app_id == package.manifest.app_id)
        }) {
            return Err(StoreError::DuplicateAppId);
        }
        Ok(package)
    }

    pub fn abort(&mut self) -> Result<(), StoreError<F::Error>> {
        if self.staging.take().is_none() {
            return Err(StoreError::NoInstall);
        }
        Ok(())
    }

    pub fn remove(
        &mut self,
        slot: usize,
        active_app_ids: &[u8],
    ) -> Result<(), StoreError<F::Error>> {
        if self.staging.is_some() {
            return Err(StoreError::Busy);
        }
        if slot >= SLOT_COUNT {
            return Err(StoreError::InvalidSlot);
        }
        if self.entries[slot].is_some_and(|entry| active_app_ids.contains(&entry.app_id)) {
            return Err(StoreError::ActiveApp);
        }
        self.flash
            .erase(slot_offset(slot), ERASE_SIZE)
            .map_err(StoreError::Flash)?;
        self.entries[slot] = None;
        Ok(())
    }

    pub fn installed(&self, slot: usize) -> Result<Option<InstalledApp>, StoreError<F::Error>> {
        if slot >= SLOT_COUNT {
            return Err(StoreError::InvalidSlot);
        }
        let Some(entry) = self.entries[slot] else {
            return Ok(None);
        };
        Ok(Some(InstalledApp {
            slot: slot as u8,
            app_id: entry.app_id,
            version: entry.version,
            package_len: entry.package_len,
        }))
    }

    pub fn package(&self, slot: usize) -> Result<Option<Package<'_>>, StoreError<F::Error>> {
        if slot >= SLOT_COUNT {
            return Err(StoreError::InvalidSlot);
        }
        let Some(entry) = self.entries[slot] else {
            return Ok(None);
        };
        let bytes = self
            .flash
            .mapped(package_offset(slot), entry.package_len as usize)
            .map_err(StoreError::Flash)?;
        Ok(Some(Package::parse(bytes)?))
    }

    pub fn into_flash(self) -> F {
        self.flash
    }
}

fn slot_offset(slot: usize) -> usize {
    slot * SLOT_SIZE
}

fn package_offset(slot: usize) -> usize {
    slot_offset(slot) + PACKAGE_OFFSET
}

fn read_slot<F: SlotFlash>(
    flash: &F,
    slot: usize,
    firmware_abi: &[u8; 32],
) -> Result<Option<SlotEntry>, F::Error> {
    let control = flash.mapped(slot_offset(slot), CONTROL_RECORD_LEN)?;
    let Some(entry) = decode_control(slot, control) else {
        return Ok(None);
    };
    if entry.package_len == 0 || entry.package_len as usize > MAX_PACKAGE_SIZE {
        return Ok(None);
    }
    let package_bytes = flash.mapped(package_offset(slot), entry.package_len as usize)?;
    if crc32(package_bytes) != entry.package_crc {
        return Ok(None);
    }
    let Ok(package) = Package::parse(package_bytes) else {
        return Ok(None);
    };
    if package.native_program().is_err() {
        return Ok(None);
    }
    if package.manifest.app_id != entry.app_id
        || package.manifest.version != entry.version
        || &package.manifest.firmware_abi != firmware_abi
    {
        return Ok(None);
    }
    Ok(Some(entry))
}

fn decode_control(slot: usize, bytes: &[u8]) -> Option<SlotEntry> {
    if bytes.len() < CONTROL_RECORD_LEN || bytes[..8] != CONTROL_MAGIC {
        return None;
    }
    if read_u16(bytes, 8)? != CONTROL_VERSION || bytes[10] as usize != slot {
        return None;
    }
    if read_u32(bytes, CONTROL_BODY_LEN)? != crc32(&bytes[..CONTROL_BODY_LEN]) {
        return None;
    }
    Some(SlotEntry {
        app_id: bytes[11],
        version: Version::new(
            read_u16(bytes, 12)?,
            read_u16(bytes, 14)?,
            read_u16(bytes, 16)?,
        ),
        package_len: read_u32(bytes, 18)?,
        package_crc: read_u32(bytes, 22)?,
    })
}

fn encode_control(slot: usize, entry: SlotEntry) -> [u8; CONTROL_RECORD_LEN] {
    let mut bytes = [0u8; CONTROL_RECORD_LEN];
    bytes[..8].copy_from_slice(&CONTROL_MAGIC);
    bytes[8..10].copy_from_slice(&CONTROL_VERSION.to_le_bytes());
    bytes[10] = slot as u8;
    bytes[11] = entry.app_id;
    bytes[12..14].copy_from_slice(&entry.version.major.to_le_bytes());
    bytes[14..16].copy_from_slice(&entry.version.minor.to_le_bytes());
    bytes[16..18].copy_from_slice(&entry.version.patch.to_le_bytes());
    bytes[18..22].copy_from_slice(&entry.package_len.to_le_bytes());
    bytes[22..26].copy_from_slice(&entry.package_crc.to_le_bytes());
    let checksum = crc32(&bytes[..CONTROL_BODY_LEN]);
    bytes[CONTROL_BODY_LEN..].copy_from_slice(&checksum.to_le_bytes());
    bytes
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let bytes = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let bytes = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::fpapp::{
        Manifest, NativeEntrypoints, NativeProgram, PackageBuilder, Version,
        PROGRAM_KIND_THUMB_ROPI,
    };
    use std::vec;
    use std::vec::Vec;

    const ABI: [u8; 32] = [0x42; 32];

    struct VecFlash {
        bytes: Vec<u8>,
    }

    impl VecFlash {
        fn erased() -> Self {
            Self {
                bytes: vec![0xff; FPAPP_REGION_SIZE],
            }
        }
    }

    impl SlotFlash for VecFlash {
        type Error = ();

        fn len(&self) -> usize {
            self.bytes.len()
        }

        fn mapped(&self, offset: usize, len: usize) -> Result<&[u8], Self::Error> {
            self.bytes.get(offset..offset + len).ok_or(())
        }

        fn erase(&mut self, offset: usize, len: usize) -> Result<(), Self::Error> {
            self.bytes
                .get_mut(offset..offset + len)
                .ok_or(())?
                .fill(0xff);
            Ok(())
        }

        fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error> {
            let destination = self.bytes.get_mut(offset..offset + data.len()).ok_or(())?;
            if destination
                .iter()
                .zip(data)
                .any(|(&old, &new)| old & new != new)
            {
                return Err(());
            }
            destination.copy_from_slice(data);
            Ok(())
        }
    }

    fn package(version: Version, program_byte: u8) -> Vec<u8> {
        package_for(102, ABI, version, program_byte)
    }

    fn package_for(
        app_id: u8,
        firmware_abi: [u8; 32],
        version: Version,
        program_byte: u8,
    ) -> Vec<u8> {
        let manifest = Manifest {
            app_id,
            version,
            program_kind: PROGRAM_KIND_THUMB_ROPI,
            name: "Sift",
            description: "Threshold sequencer",
            author: "thorinside",
            channels: 2,
            color_rgb: 0x00ff_00ff,
            icon: 13,
            parameter_count: 0,
            persistent_state_bytes: 128,
            execution_units_per_event: 10_000,
            capabilities: 3,
            firmware_abi,
        };
        let mut output = [0u8; 256];
        let image = [program_byte; 16];
        let mut native_output = [0u8; 64];
        let native_program = NativeProgram::encode(
            NativeEntrypoints {
                required_bytes: 0,
                init: 4,
                poll: 8,
                drop: 12,
            },
            &image,
            &mut native_output,
        )
        .unwrap();
        PackageBuilder::new(manifest, native_program)
            .encode(&mut output)
            .unwrap()
            .to_vec()
    }

    fn install(store: &mut SlotStore<VecFlash>, slot: usize, bytes: &[u8]) -> InstalledApp {
        store.begin_install(slot, bytes.len(), &[], 0).unwrap();
        store.write_chunk(0, bytes, 0).unwrap();
        store.commit().unwrap()
    }

    #[test]
    fn interrupted_replacement_leaves_only_that_slot_empty() {
        let version_one = package(Version::new(1, 0, 0), 0x11);
        let version_two = package(Version::new(2, 0, 0), 0x22);
        let other = package_for(103, ABI, Version::new(1, 0, 0), 0x33);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        install(&mut store, 0, &version_one);
        install(&mut store, 1, &other);

        store.begin_install(0, version_two.len(), &[], 0).unwrap();
        store.write_chunk(0, &version_two[..32], 0).unwrap();
        let reopened = SlotStore::open(store.into_flash(), ABI).unwrap();

        assert_eq!(reopened.installed(0).unwrap(), None);
        assert_eq!(reopened.installed(1).unwrap().unwrap().app_id, 103);
    }

    #[test]
    fn completed_install_survives_reopen() {
        let bytes = package(Version::new(2, 1, 3), 0x22);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        let installed = install(&mut store, 3, &bytes);
        assert_eq!(installed.slot, 3);

        let reopened = SlotStore::open(store.into_flash(), ABI).unwrap();
        assert_eq!(reopened.installed(3).unwrap(), Some(installed));
        assert_eq!(
            reopened
                .package(3)
                .unwrap()
                .unwrap()
                .native_program()
                .unwrap()
                .image,
            &[0x22; 16]
        );
    }

    #[test]
    fn fully_uploaded_package_can_be_checked_before_it_is_published() {
        let bytes = package(Version::new(1, 2, 3), 0x22);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        store.begin_install(0, bytes.len(), &[], 0).unwrap();
        store.write_chunk(0, &bytes, 0).unwrap();

        let staged = store.staged_package().unwrap();
        assert_eq!(staged.manifest.app_id, 102);
        assert_eq!(staged.manifest.version, Version::new(1, 2, 3));
        assert_eq!(store.installed(0).unwrap(), None);
    }

    #[test]
    fn incompatible_package_never_becomes_installed() {
        let bytes = package_for(102, [0x99; 32], Version::new(1, 0, 0), 0x11);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        store.begin_install(0, bytes.len(), &[], 0).unwrap();
        store.write_chunk(0, &bytes, 0).unwrap();

        assert_eq!(store.commit(), Err(StoreError::IncompatibleFirmware));
        store.abort().unwrap();
        let reopened = SlotStore::open(store.into_flash(), ABI).unwrap();
        assert_eq!(reopened.installed(0).unwrap(), None);
    }

    #[test]
    fn active_apps_cannot_be_replaced_or_removed() {
        let bytes = package(Version::new(1, 0, 0), 0x11);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        install(&mut store, 0, &bytes);

        assert_eq!(
            store.begin_install(0, bytes.len(), &[102], 0),
            Err(StoreError::ActiveApp)
        );
        assert_eq!(store.remove(0, &[102]), Err(StoreError::ActiveApp));
        assert!(store.installed(0).unwrap().is_some());
    }

    #[test]
    fn abandoned_staging_expires_instead_of_wedging_the_slot() {
        let bytes = package(Version::new(1, 0, 0), 0x11);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();

        // An upload starts and then the uploader vanishes part-way through.
        store.begin_install(0, bytes.len(), &[], 1_000).unwrap();
        store.write_chunk(0, &bytes[..4], 1_100).unwrap();

        // While it still looks live, the slot stays reserved.
        assert_eq!(
            store.begin_install(0, bytes.len(), &[], 1_100 + STAGING_TIMEOUT_MS - 1),
            Err(StoreError::Busy)
        );

        // Past the timeout a fresh install supersedes it rather than being
        // locked out until the next power cycle.
        store
            .begin_install(0, bytes.len(), &[], 1_100 + STAGING_TIMEOUT_MS)
            .unwrap();
        store.write_chunk(0, &bytes, 2_000).unwrap();
        store.commit().unwrap();
        assert!(store.installed(0).unwrap().is_some());
    }

    #[test]
    fn staging_timeout_is_measured_from_the_last_chunk() {
        let bytes = package(Version::new(1, 0, 0), 0x11);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        store.begin_install(0, bytes.len(), &[], 0).unwrap();

        // A slow but live upload keeps refreshing the deadline, so it must not
        // be evicted just because it started long ago.
        let mut now = 0;
        for chunk in 0..4 {
            now = chunk * (STAGING_TIMEOUT_MS - 1);
            store
                .write_chunk(chunk as usize * 4, &bytes[chunk as usize * 4..][..4], now)
                .unwrap();
        }
        assert_eq!(
            store.begin_install(0, bytes.len(), &[], now + 1),
            Err(StoreError::Busy)
        );
    }

    #[test]
    fn chunks_are_sequential_and_removal_survives_reopen() {
        let bytes = package(Version::new(1, 0, 0), 0x11);
        let mut store = SlotStore::open(VecFlash::erased(), ABI).unwrap();
        store.begin_install(2, bytes.len(), &[], 0).unwrap();
        assert_eq!(
            store.write_chunk(1, &bytes[..4], 0),
            Err(StoreError::UnexpectedOffset { expected: 0 })
        );
        store.write_chunk(0, &bytes, 0).unwrap();
        store.commit().unwrap();
        store.remove(2, &[]).unwrap();

        let reopened = SlotStore::open(store.into_flash(), ABI).unwrap();
        assert_eq!(reopened.installed(2).unwrap(), None);
    }
}
