//! Installable Faderpunk application package format.
//!
//! The public interface parses and builds bounded, allocation-free `.fpapp`
//! containers. Firmware storage and execution live behind separate seams.

pub const MAGIC: [u8; 8] = *b"FPAPP\0\r\n";
pub const CONTAINER_VERSION: u16 = 0;
pub const RUNTIME_ABI_VERSION: u16 = 1;
pub const PROGRAM_KIND_THUMB_ROPI: u16 = 1;

pub const NATIVE_PROGRAM_MAGIC: [u8; 4] = *b"FPN0";
pub const NATIVE_PROGRAM_VERSION: u16 = 0;
pub const NATIVE_PROGRAM_HEADER_LEN: usize = 28;

/// Compare wrapping 32-bit event sequence numbers using serial-number
/// arithmetic. A runtime queue contains far fewer than half the sequence
/// space, so values in the forward half are unambiguously newer.
pub const fn sequence_is_after(candidate: u32, cursor: u32) -> bool {
    let distance = candidate.wrapping_sub(cursor);
    distance != 0 && distance < (1 << 31)
}

const FIXED_HEADER_LEN: usize = 24;
const SECTION_DESCRIPTOR_LEN: usize = 12;
const SECTION_MANIFEST: u16 = 1;
const SECTION_PROGRAM: u16 = 2;
const SECTION_MANUAL: u16 = 3;
const SECTION_SETUP: u16 = 4;
const SECTION_SETTINGS: u16 = 5;
const SECTION_SIGNING: u16 = 6;
const SECTION_REQUIRED: u16 = 1;
const MAX_SECTION_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest<'a> {
    pub app_id: u8,
    pub version: Version,
    pub program_kind: u16,
    pub name: &'a str,
    pub description: &'a str,
    pub author: &'a str,
    pub channels: u8,
    pub color_rgb: u32,
    pub icon: u8,
    pub parameter_count: u8,
    pub persistent_state_bytes: u16,
    pub execution_units_per_event: u32,
    pub capabilities: u32,
    pub firmware_abi: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Package<'a> {
    pub manifest: Manifest<'a>,
    pub program: &'a [u8],
    /// Markdown shown by the Configurator as the app manual.
    pub manual: Option<&'a str>,
    /// Markdown shown before installation for wiring and setup instructions.
    pub setup: Option<&'a str>,
    /// UTF-8 JSON Schema consumed by the Configurator settings hook.
    pub settings: Option<&'a str>,
    pub signing: Option<&'a [u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageError {
    Truncated,
    InvalidMagic,
    UnsupportedContainerVersion,
    UnsupportedRuntimeAbi,
    LengthMismatch,
    InvalidSectionTable,
    InvalidSection,
    OverlappingSections,
    MissingManifest,
    MissingProgram,
    DuplicateSection,
    UnknownRequiredSection,
    CrcMismatch,
    InvalidManifest,
    InvalidUtf8,
    BufferTooSmall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeEntrypoints {
    pub required_bytes: u32,
    pub init: u32,
    pub poll: u32,
    pub drop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeProgram<'a> {
    pub entrypoints: NativeEntrypoints,
    pub image: &'a [u8],
}

impl<'a> NativeProgram<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageError> {
        if bytes.len() < NATIVE_PROGRAM_HEADER_LEN {
            return Err(PackageError::Truncated);
        }
        if bytes[..4] != NATIVE_PROGRAM_MAGIC
            || read_u16(bytes, 4)? != NATIVE_PROGRAM_VERSION
            || read_u16(bytes, 6)? as usize != NATIVE_PROGRAM_HEADER_LEN
        {
            return Err(PackageError::InvalidSection);
        }
        let entrypoints = NativeEntrypoints {
            required_bytes: read_u32(bytes, 8)?,
            init: read_u32(bytes, 12)?,
            poll: read_u32(bytes, 16)?,
            drop: read_u32(bytes, 20)?,
        };
        let image_len = read_u32(bytes, 24)? as usize;
        if image_len != bytes.len() - NATIVE_PROGRAM_HEADER_LEN {
            return Err(PackageError::LengthMismatch);
        }
        for offset in [
            entrypoints.required_bytes,
            entrypoints.init,
            entrypoints.poll,
            entrypoints.drop,
        ] {
            if offset as usize >= image_len || !offset.is_multiple_of(2) {
                return Err(PackageError::InvalidSection);
            }
        }
        Ok(Self {
            entrypoints,
            image: &bytes[NATIVE_PROGRAM_HEADER_LEN..],
        })
    }

    pub fn encode<'output>(
        entrypoints: NativeEntrypoints,
        image: &[u8],
        output: &'output mut [u8],
    ) -> Result<&'output [u8], PackageError> {
        let total_len = NATIVE_PROGRAM_HEADER_LEN
            .checked_add(image.len())
            .ok_or(PackageError::BufferTooSmall)?;
        if total_len > output.len() {
            return Err(PackageError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&NATIVE_PROGRAM_MAGIC);
        write_u16(output, 4, NATIVE_PROGRAM_VERSION)?;
        write_u16(output, 6, NATIVE_PROGRAM_HEADER_LEN as u16)?;
        write_u32(output, 8, entrypoints.required_bytes)?;
        write_u32(output, 12, entrypoints.init)?;
        write_u32(output, 16, entrypoints.poll)?;
        write_u32(output, 20, entrypoints.drop)?;
        write_u32(
            output,
            24,
            u32::try_from(image.len()).map_err(|_| PackageError::BufferTooSmall)?,
        )?;
        output[NATIVE_PROGRAM_HEADER_LEN..total_len].copy_from_slice(image);
        let encoded = &output[..total_len];
        NativeProgram::<'output>::parse(encoded)?;
        Ok(encoded)
    }
}

#[derive(Clone, Copy)]
struct Section<'a> {
    data: &'a [u8],
}

pub struct PackageBuilder<'a> {
    manifest: Manifest<'a>,
    program: &'a [u8],
    manual: Option<&'a str>,
    setup: Option<&'a str>,
    settings: Option<&'a str>,
    signing: Option<&'a [u8]>,
}

impl<'a> PackageBuilder<'a> {
    pub const fn new(manifest: Manifest<'a>, program: &'a [u8]) -> Self {
        Self {
            manifest,
            program,
            manual: None,
            setup: None,
            settings: None,
            signing: None,
        }
    }

    pub const fn with_manual(mut self, manual: &'a str) -> Self {
        self.manual = Some(manual);
        self
    }

    pub const fn with_setup(mut self, setup: &'a str) -> Self {
        self.setup = Some(setup);
        self
    }

    pub const fn with_settings(mut self, settings: &'a str) -> Self {
        self.settings = Some(settings);
        self
    }

    pub const fn with_signing(mut self, signing: &'a [u8]) -> Self {
        self.signing = Some(signing);
        self
    }

    pub fn encode<'output>(
        &self,
        output: &'output mut [u8],
    ) -> Result<&'output [u8], PackageError> {
        validate_manifest(&self.manifest)?;

        let section_count = 2
            + usize::from(self.manual.is_some())
            + usize::from(self.setup.is_some())
            + usize::from(self.settings.is_some())
            + usize::from(self.signing.is_some());
        let table_end = FIXED_HEADER_LEN + section_count * SECTION_DESCRIPTOR_LEN;
        if output.len() < table_end {
            return Err(PackageError::BufferTooSmall);
        }

        let manifest_len = encode_manifest(&self.manifest, &mut output[table_end..])?;
        let manifest_end = table_end
            .checked_add(manifest_len)
            .ok_or(PackageError::BufferTooSmall)?;
        let program_offset = align4(manifest_end).ok_or(PackageError::BufferTooSmall)?;
        let mut next_offset = program_offset
            .checked_add(self.program.len())
            .ok_or(PackageError::BufferTooSmall)?;
        if next_offset > output.len() {
            return Err(PackageError::BufferTooSmall);
        }

        output[manifest_end..program_offset].fill(0);
        output[program_offset..next_offset].copy_from_slice(self.program);

        let optional_sections = [
            self.manual.map(|value| (SECTION_MANUAL, value.as_bytes())),
            self.setup.map(|value| (SECTION_SETUP, value.as_bytes())),
            self.settings
                .map(|value| (SECTION_SETTINGS, value.as_bytes())),
            self.signing.map(|value| (SECTION_SIGNING, value)),
        ];
        let mut descriptor_index = 2;
        for (kind, data) in optional_sections.into_iter().flatten() {
            let section_offset = align4(next_offset).ok_or(PackageError::BufferTooSmall)?;
            let section_end = section_offset
                .checked_add(data.len())
                .ok_or(PackageError::BufferTooSmall)?;
            if section_end > output.len() {
                return Err(PackageError::BufferTooSmall);
            }
            output[next_offset..section_offset].fill(0);
            output[section_offset..section_end].copy_from_slice(data);
            write_section_descriptor(
                output,
                descriptor_index,
                kind,
                0,
                section_offset,
                data.len(),
            )?;
            descriptor_index += 1;
            next_offset = section_end;
        }
        let total_len = next_offset;

        output[..8].copy_from_slice(&MAGIC);
        write_u16(output, 8, CONTAINER_VERSION)?;
        write_u16(output, 10, RUNTIME_ABI_VERSION)?;
        write_u32(
            output,
            12,
            u32::try_from(total_len).map_err(|_| PackageError::BufferTooSmall)?,
        )?;
        write_u16(output, 16, section_count as u16)?;
        write_u16(output, 18, table_end as u16)?;
        write_u32(output, 20, crc32(&output[table_end..total_len]))?;

        write_section_descriptor(
            output,
            0,
            SECTION_MANIFEST,
            SECTION_REQUIRED,
            table_end,
            manifest_len,
        )?;
        write_section_descriptor(
            output,
            1,
            SECTION_PROGRAM,
            SECTION_REQUIRED,
            program_offset,
            self.program.len(),
        )?;

        Ok(&output[..total_len])
    }
}

impl<'a> Package<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageError> {
        if bytes.len() < FIXED_HEADER_LEN {
            return Err(PackageError::Truncated);
        }
        if bytes[..MAGIC.len()] != MAGIC {
            return Err(PackageError::InvalidMagic);
        }

        let container_version = read_u16(bytes, 8)?;
        if container_version != CONTAINER_VERSION {
            return Err(PackageError::UnsupportedContainerVersion);
        }
        let runtime_abi = read_u16(bytes, 10)?;
        if runtime_abi != RUNTIME_ABI_VERSION {
            return Err(PackageError::UnsupportedRuntimeAbi);
        }

        let total_len = read_u32(bytes, 12)? as usize;
        if total_len != bytes.len() {
            return Err(PackageError::LengthMismatch);
        }
        let section_count = read_u16(bytes, 16)? as usize;
        if section_count > MAX_SECTION_COUNT {
            return Err(PackageError::InvalidSectionTable);
        }
        let table_end = read_u16(bytes, 18)? as usize;
        let expected_table_end = FIXED_HEADER_LEN
            .checked_add(
                section_count
                    .checked_mul(SECTION_DESCRIPTOR_LEN)
                    .ok_or(PackageError::InvalidSectionTable)?,
            )
            .ok_or(PackageError::InvalidSectionTable)?;
        if table_end != expected_table_end || table_end > bytes.len() {
            return Err(PackageError::InvalidSectionTable);
        }

        let expected_crc = read_u32(bytes, 20)?;
        if crc32(&bytes[table_end..]) != expected_crc {
            return Err(PackageError::CrcMismatch);
        }

        let mut manifest_section = None;
        let mut program_section = None;
        let mut manual_section = None;
        let mut setup_section = None;
        let mut settings_section = None;
        let mut signing_section = None;

        for index in 0..section_count {
            let descriptor = FIXED_HEADER_LEN + index * SECTION_DESCRIPTOR_LEN;
            let kind = read_u16(bytes, descriptor)?;
            let flags = read_u16(bytes, descriptor + 2)?;
            let offset = read_u32(bytes, descriptor + 4)? as usize;
            let len = read_u32(bytes, descriptor + 8)? as usize;
            let end = offset
                .checked_add(len)
                .ok_or(PackageError::InvalidSection)?;
            if offset < table_end || !offset.is_multiple_of(4) || end > bytes.len() {
                return Err(PackageError::InvalidSection);
            }
            for previous_index in 0..index {
                let previous_descriptor =
                    FIXED_HEADER_LEN + previous_index * SECTION_DESCRIPTOR_LEN;
                let previous_offset = read_u32(bytes, previous_descriptor + 4)? as usize;
                let previous_len = read_u32(bytes, previous_descriptor + 8)? as usize;
                let previous_end = previous_offset
                    .checked_add(previous_len)
                    .ok_or(PackageError::InvalidSection)?;
                if offset < previous_end && previous_offset < end {
                    return Err(PackageError::OverlappingSections);
                }
            }
            let section = Section {
                data: &bytes[offset..end],
            };

            match kind {
                SECTION_MANIFEST => set_once(&mut manifest_section, section)?,
                SECTION_PROGRAM => set_once(&mut program_section, section)?,
                SECTION_MANUAL => set_once(&mut manual_section, section)?,
                SECTION_SETUP => set_once(&mut setup_section, section)?,
                SECTION_SETTINGS => set_once(&mut settings_section, section)?,
                SECTION_SIGNING => set_once(&mut signing_section, section)?,
                _ if flags & SECTION_REQUIRED != 0 => {
                    return Err(PackageError::UnknownRequiredSection)
                }
                _ => {}
            }
        }

        let manifest_section = manifest_section.ok_or(PackageError::MissingManifest)?;
        let program_section = program_section.ok_or(PackageError::MissingProgram)?;
        let manifest = parse_manifest(manifest_section.data)?;
        let manual = manual_section
            .map(|section| core::str::from_utf8(section.data))
            .transpose()
            .map_err(|_| PackageError::InvalidUtf8)?;
        let setup = setup_section
            .map(|section| core::str::from_utf8(section.data))
            .transpose()
            .map_err(|_| PackageError::InvalidUtf8)?;
        let settings = settings_section
            .map(|section| core::str::from_utf8(section.data))
            .transpose()
            .map_err(|_| PackageError::InvalidUtf8)?;

        Ok(Self {
            manifest,
            program: program_section.data,
            manual,
            setup,
            settings,
            signing: signing_section.map(|section| section.data),
        })
    }

    pub fn native_program(&self) -> Result<NativeProgram<'a>, PackageError> {
        if self.manifest.program_kind != PROGRAM_KIND_THUMB_ROPI {
            return Err(PackageError::InvalidManifest);
        }
        NativeProgram::parse(self.program)
    }
}

fn set_once<'a>(slot: &mut Option<Section<'a>>, section: Section<'a>) -> Result<(), PackageError> {
    if slot.replace(section).is_some() {
        Err(PackageError::DuplicateSection)
    } else {
        Ok(())
    }
}

fn parse_manifest(bytes: &[u8]) -> Result<Manifest<'_>, PackageError> {
    let mut decoder = minicbor::Decoder::new(bytes);
    let fields = decoder
        .map()
        .map_err(|_| PackageError::InvalidManifest)?
        .ok_or(PackageError::InvalidManifest)?;

    let mut app_id = None;
    let mut version = None;
    let mut program_kind = None;
    let mut name = None;
    let mut description = None;
    let mut author = None;
    let mut channels = None;
    let mut color_rgb = None;
    let mut icon = None;
    let mut parameter_count = None;
    let mut persistent_state_bytes = None;
    let mut execution_units_per_event = None;
    let mut capabilities = None;
    let mut firmware_abi = None;

    for _ in 0..fields {
        let key = decoder.u32().map_err(|_| PackageError::InvalidManifest)?;
        match key {
            0 => app_id = Some(decoder.u8().map_err(|_| PackageError::InvalidManifest)?),
            1 => {
                let len = decoder.array().map_err(|_| PackageError::InvalidManifest)?;
                if len != Some(3) {
                    return Err(PackageError::InvalidManifest);
                }
                version = Some(Version::new(
                    decoder.u16().map_err(|_| PackageError::InvalidManifest)?,
                    decoder.u16().map_err(|_| PackageError::InvalidManifest)?,
                    decoder.u16().map_err(|_| PackageError::InvalidManifest)?,
                ));
            }
            2 => program_kind = Some(decoder.u16().map_err(|_| PackageError::InvalidManifest)?),
            3 => name = Some(decoder.str().map_err(|_| PackageError::InvalidManifest)?),
            4 => description = Some(decoder.str().map_err(|_| PackageError::InvalidManifest)?),
            5 => author = Some(decoder.str().map_err(|_| PackageError::InvalidManifest)?),
            6 => channels = Some(decoder.u8().map_err(|_| PackageError::InvalidManifest)?),
            7 => color_rgb = Some(decoder.u32().map_err(|_| PackageError::InvalidManifest)?),
            8 => icon = Some(decoder.u8().map_err(|_| PackageError::InvalidManifest)?),
            9 => {
                let len = decoder
                    .array()
                    .map_err(|_| PackageError::InvalidManifest)?
                    .ok_or(PackageError::InvalidManifest)?;
                let count = u8::try_from(len).map_err(|_| PackageError::InvalidManifest)?;
                for _ in 0..len {
                    decoder.skip().map_err(|_| PackageError::InvalidManifest)?;
                }
                parameter_count = Some(count);
            }
            10 => {
                persistent_state_bytes =
                    Some(decoder.u16().map_err(|_| PackageError::InvalidManifest)?)
            }
            11 => {
                execution_units_per_event =
                    Some(decoder.u32().map_err(|_| PackageError::InvalidManifest)?)
            }
            12 => capabilities = Some(decoder.u32().map_err(|_| PackageError::InvalidManifest)?),
            13 => {
                firmware_abi = Some(
                    decoder
                        .bytes()
                        .map_err(|_| PackageError::InvalidManifest)?
                        .try_into()
                        .map_err(|_| PackageError::InvalidManifest)?,
                )
            }
            _ => decoder.skip().map_err(|_| PackageError::InvalidManifest)?,
        }
    }

    let manifest = Manifest {
        app_id: app_id.ok_or(PackageError::InvalidManifest)?,
        version: version.ok_or(PackageError::InvalidManifest)?,
        program_kind: program_kind.ok_or(PackageError::InvalidManifest)?,
        name: name.ok_or(PackageError::InvalidManifest)?,
        description: description.ok_or(PackageError::InvalidManifest)?,
        author: author.ok_or(PackageError::InvalidManifest)?,
        channels: channels.ok_or(PackageError::InvalidManifest)?,
        color_rgb: color_rgb.ok_or(PackageError::InvalidManifest)?,
        icon: icon.ok_or(PackageError::InvalidManifest)?,
        parameter_count: parameter_count.ok_or(PackageError::InvalidManifest)?,
        persistent_state_bytes: persistent_state_bytes.ok_or(PackageError::InvalidManifest)?,
        execution_units_per_event: execution_units_per_event
            .ok_or(PackageError::InvalidManifest)?,
        capabilities: capabilities.ok_or(PackageError::InvalidManifest)?,
        firmware_abi: firmware_abi.ok_or(PackageError::InvalidManifest)?,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest<'_>) -> Result<(), PackageError> {
    if !(100..=255).contains(&manifest.app_id)
        || !(1..=16).contains(&manifest.channels)
        || manifest.parameter_count as usize > crate::APP_MAX_PARAMS
        || manifest.name.is_empty()
        || manifest.name.len() > 32
        || !manifest.name.is_ascii()
        || manifest.description.is_empty()
        || manifest.description.len() > 96
        || !manifest.description.is_ascii()
        || manifest.author.is_empty()
        || manifest.author.len() > 64
    {
        return Err(PackageError::InvalidManifest);
    }
    Ok(())
}

struct SliceWriter<'a> {
    output: &'a mut [u8],
    position: usize,
}

impl minicbor::encode::Write for SliceWriter<'_> {
    type Error = PackageError;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or(PackageError::BufferTooSmall)?;
        let target = self
            .output
            .get_mut(self.position..end)
            .ok_or(PackageError::BufferTooSmall)?;
        target.copy_from_slice(bytes);
        self.position = end;
        Ok(())
    }
}

fn encode_manifest(manifest: &Manifest<'_>, output: &mut [u8]) -> Result<usize, PackageError> {
    let mut writer = SliceWriter {
        output,
        position: 0,
    };
    {
        let mut encoder = minicbor::Encoder::new(&mut writer);
        encoder
            .map(14)
            .and_then(|encoder| encoder.u8(0))
            .and_then(|encoder| encoder.u8(manifest.app_id))
            .and_then(|encoder| encoder.u8(1))
            .and_then(|encoder| encoder.array(3))
            .and_then(|encoder| encoder.u16(manifest.version.major))
            .and_then(|encoder| encoder.u16(manifest.version.minor))
            .and_then(|encoder| encoder.u16(manifest.version.patch))
            .and_then(|encoder| encoder.u8(2))
            .and_then(|encoder| encoder.u16(manifest.program_kind))
            .and_then(|encoder| encoder.u8(3))
            .and_then(|encoder| encoder.str(manifest.name))
            .and_then(|encoder| encoder.u8(4))
            .and_then(|encoder| encoder.str(manifest.description))
            .and_then(|encoder| encoder.u8(5))
            .and_then(|encoder| encoder.str(manifest.author))
            .and_then(|encoder| encoder.u8(6))
            .and_then(|encoder| encoder.u8(manifest.channels))
            .and_then(|encoder| encoder.u8(7))
            .and_then(|encoder| encoder.u32(manifest.color_rgb))
            .and_then(|encoder| encoder.u8(8))
            .and_then(|encoder| encoder.u8(manifest.icon))
            .and_then(|encoder| encoder.u8(9))
            .and_then(|encoder| encoder.array(manifest.parameter_count.into()))
            .map_err(|error| error.into_write().unwrap_or(PackageError::InvalidManifest))?;
        for _ in 0..manifest.parameter_count {
            encoder
                .null()
                .map_err(|error| error.into_write().unwrap_or(PackageError::InvalidManifest))?;
        }
        encoder
            .u8(10)
            .and_then(|encoder| encoder.u16(manifest.persistent_state_bytes))
            .and_then(|encoder| encoder.u8(11))
            .and_then(|encoder| encoder.u32(manifest.execution_units_per_event))
            .and_then(|encoder| encoder.u8(12))
            .and_then(|encoder| encoder.u32(manifest.capabilities))
            .and_then(|encoder| encoder.u8(13))
            .and_then(|encoder| encoder.bytes(&manifest.firmware_abi))
            .map_err(|error| error.into_write().unwrap_or(PackageError::InvalidManifest))?;
    }
    Ok(writer.position)
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3)
}

fn write_section_descriptor(
    output: &mut [u8],
    index: usize,
    kind: u16,
    flags: u16,
    offset: usize,
    len: usize,
) -> Result<(), PackageError> {
    let descriptor = FIXED_HEADER_LEN + index * SECTION_DESCRIPTOR_LEN;
    write_u16(output, descriptor, kind)?;
    write_u16(output, descriptor + 2, flags)?;
    write_u32(
        output,
        descriptor + 4,
        u32::try_from(offset).map_err(|_| PackageError::BufferTooSmall)?,
    )?;
    write_u32(
        output,
        descriptor + 8,
        u32::try_from(len).map_err(|_| PackageError::BufferTooSmall)?,
    )
}

fn write_u16(output: &mut [u8], offset: usize, value: u16) -> Result<(), PackageError> {
    output
        .get_mut(offset..offset + 2)
        .ok_or(PackageError::BufferTooSmall)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(output: &mut [u8], offset: usize, value: u32) -> Result<(), PackageError> {
    output
        .get_mut(offset..offset + 4)
        .ok_or(PackageError::BufferTooSmall)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(PackageError::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(PackageError::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let polynomial = 0xedb8_8320 & (0u32.wrapping_sub(crc & 1));
            crc = (crc >> 1) ^ polynomial;
        }
    }
    !crc
}

/// Converts a 40-digit Git revision into the fixed firmware compatibility ID.
///
/// Git's 20-byte object ID is kept verbatim. Three salted CRCs fill the
/// remaining bytes so the on-wire field remains 32 bytes without requiring a
/// cryptographic hash implementation in firmware tooling.
pub fn firmware_abi_from_revision(revision: &str) -> Option<[u8; 32]> {
    if revision.len() != 40 || !revision.is_ascii() {
        return None;
    }
    let mut output = [0u8; 32];
    for (index, byte) in output[..20].iter_mut().enumerate() {
        *byte = parse_hex_byte(&revision.as_bytes()[index * 2..index * 2 + 2])?;
    }
    for salt in 0..3u8 {
        let mut crc = 0xffff_ffffu32;
        for byte in revision.bytes().chain(core::iter::once(salt)) {
            crc ^= byte as u32;
            for _ in 0..8 {
                let polynomial = 0xedb8_8320 & (0u32.wrapping_sub(crc & 1));
                crc = (crc >> 1) ^ polynomial;
            }
        }
        let offset = 20 + salt as usize * 4;
        output[offset..offset + 4].copy_from_slice(&(!crc).to_le_bytes());
    }
    Some(output)
}

/// Parses the 64-digit compatibility ID used for explicit development builds.
pub fn firmware_abi_from_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        return None;
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = parse_hex_byte(&value.as_bytes()[index * 2..index * 2 + 2])?;
    }
    Some(output)
}

fn parse_hex_byte(value: &[u8]) -> Option<u8> {
    let high = parse_hex_nibble(*value.first()?)?;
    let low = parse_hex_nibble(*value.get(1)?)?;
    Some((high << 4) | low)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_sequence_comparison_survives_wraparound() {
        assert!(sequence_is_after(1, u32::MAX));
        assert!(sequence_is_after(2, u32::MAX));
        assert!(!sequence_is_after(u32::MAX, 1));
        assert!(!sequence_is_after(7, 7));
    }

    // Hand-authored from the container-v0/runtime-v1 layout in
    // docs/fpapp-native-rfc.md. Keeping
    // this independent of the builder catches drift in offsets, endian order,
    // CBOR field numbers, alignment, and the CRC-covered byte range.
    const GOLDEN_SIFT_PACKAGE: &[u8] = &[
        0x46, 0x50, 0x41, 0x50, 0x50, 0x00, 0x0d, 0x0a, 0x00, 0x00, 0x01, 0x00, 0x90, 0x00, 0x00,
        0x00, 0x02, 0x00, 0x30, 0x00, 0xcb, 0x32, 0x91, 0xf9, 0x01, 0x00, 0x01, 0x00, 0x30, 0x00,
        0x00, 0x00, 0x5a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x8c, 0x00, 0x00, 0x00, 0x04,
        0x00, 0x00, 0x00, 0xae, 0x00, 0x18, 0x66, 0x01, 0x83, 0x01, 0x00, 0x00, 0x02, 0x00, 0x03,
        0x64, 0x53, 0x69, 0x66, 0x74, 0x04, 0x69, 0x54, 0x68, 0x72, 0x65, 0x73, 0x68, 0x6f, 0x6c,
        0x64, 0x05, 0x64, 0x4e, 0x65, 0x61, 0x6c, 0x06, 0x02, 0x07, 0x1a, 0x00, 0xff, 0x00, 0xff,
        0x08, 0x0d, 0x09, 0x80, 0x0a, 0x18, 0x40, 0x0b, 0x19, 0x27, 0x10, 0x0c, 0x03, 0x0d, 0x58,
        0x20, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
        0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
        0x11, 0x11, 0x11, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn parses_a_golden_container_v0_runtime_v1_package() {
        let package = Package::parse(GOLDEN_SIFT_PACKAGE).unwrap();

        assert_eq!(package.manifest.app_id, 102);
        assert_eq!(package.manifest.version, Version::new(1, 0, 0));
        assert_eq!(package.manifest.program_kind, 0);
        assert_eq!(package.manifest.name, "Sift");
        assert_eq!(package.manifest.description, "Threshold");
        assert_eq!(package.manifest.author, "Neal");
        assert_eq!(package.manifest.channels, 2);
        assert_eq!(package.manifest.color_rgb, 0x00ff_00ff);
        assert_eq!(package.manifest.icon, 13);
        assert_eq!(package.manifest.parameter_count, 0);
        assert_eq!(package.manifest.persistent_state_bytes, 64);
        assert_eq!(package.manifest.execution_units_per_event, 10_000);
        assert_eq!(package.manifest.capabilities, 3);
        assert_eq!(package.manifest.firmware_abi, [0x11; 32]);
        assert_eq!(package.program, &[0x13, 0x00, 0x00, 0x00]);
        assert_eq!(package.manual, None);
        assert_eq!(package.setup, None);
        assert_eq!(package.settings, None);
        assert_eq!(package.signing, None);
    }

    #[test]
    fn builder_reproduces_the_golden_package() {
        let manifest = Manifest {
            app_id: 102,
            version: Version::new(1, 0, 0),
            program_kind: 0,
            name: "Sift",
            description: "Threshold",
            author: "Neal",
            channels: 2,
            color_rgb: 0x00ff_00ff,
            icon: 13,
            parameter_count: 0,
            persistent_state_bytes: 64,
            execution_units_per_event: 10_000,
            capabilities: 3,
            firmware_abi: [0x11; 32],
        };
        let mut output = [0u8; 160];

        let encoded = PackageBuilder::new(manifest, &[0x13, 0x00, 0x00, 0x00])
            .encode(&mut output)
            .unwrap();

        assert_eq!(encoded, GOLDEN_SIFT_PACKAGE);
    }

    #[test]
    fn builder_round_trips_configurator_hooks() {
        let manifest = Manifest {
            app_id: 102,
            version: Version::new(1, 2, 3),
            program_kind: 0,
            name: "Sift",
            description: "Threshold sequencer",
            author: "thorinside",
            channels: 2,
            color_rgb: 0x00ff_00ff,
            icon: 13,
            parameter_count: 3,
            persistent_state_bytes: 64,
            execution_units_per_event: 10_000,
            capabilities: 3,
            firmware_abi: [0x22; 32],
        };
        let mut output = [0u8; 512];

        let encoded = PackageBuilder::new(manifest, &[1, 2, 3, 4])
            .with_manual("# Sift\n\nPlayer documentation.")
            .with_setup("Connect a clock to channel 1.")
            .with_settings(r#"{"type":"object","properties":{}}"#)
            .with_signing(&[0xaa, 0xbb])
            .encode(&mut output)
            .unwrap();
        let package = Package::parse(encoded).unwrap();

        assert_eq!(package.manual, Some("# Sift\n\nPlayer documentation."));
        assert_eq!(package.setup, Some("Connect a clock to channel 1."));
        assert_eq!(
            package.settings,
            Some(r#"{"type":"object","properties":{}}"#)
        );
        assert_eq!(package.signing, Some(&[0xaa, 0xbb][..]));
        assert_eq!(package.manifest.parameter_count, 3);
    }

    #[test]
    fn parser_rejects_overlapping_sections() {
        let mut corrupted = [0u8; GOLDEN_SIFT_PACKAGE.len()];
        corrupted.copy_from_slice(GOLDEN_SIFT_PACKAGE);
        // Point the program section into the manifest section, then repair the
        // body CRC so overlap detection is the reason for rejection.
        corrupted[40..44].copy_from_slice(&0x30u32.to_le_bytes());
        let table_end = 0x30;
        let crc = crc32(&corrupted[table_end..]);
        corrupted[20..24].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            Package::parse(&corrupted),
            Err(PackageError::OverlappingSections)
        );
    }

    #[test]
    fn native_program_round_trips_entrypoint_offsets() {
        let entrypoints = NativeEntrypoints {
            required_bytes: 0,
            init: 4,
            poll: 8,
            drop: 12,
        };
        let image = [0u8; 16];
        let mut output = [0u8; 64];

        let encoded = NativeProgram::encode(entrypoints, &image, &mut output).unwrap();
        let parsed = NativeProgram::parse(encoded).unwrap();

        assert_eq!(parsed.entrypoints, entrypoints);
        assert_eq!(parsed.image, image);
    }

    #[test]
    fn firmware_abi_contains_the_exact_git_object_id() {
        let revision = "c893125b9c97e1332c258e834d5592aae76b6562";
        let abi = firmware_abi_from_revision(revision).unwrap();

        assert_eq!(
            &abi[..20],
            &[
                0xc8, 0x93, 0x12, 0x5b, 0x9c, 0x97, 0xe1, 0x33, 0x2c, 0x25, 0x8e, 0x83, 0x4d, 0x55,
                0x92, 0xaa, 0xe7, 0x6b, 0x65, 0x62,
            ]
        );
        assert_ne!(&abi[20..], &[0; 12]);
    }

    #[test]
    fn explicit_firmware_abi_hex_round_trips() {
        let text = "00112233445566778899aabbccddeeffffeeddccbbaa99887766554433221100";
        let abi = firmware_abi_from_hex(text).unwrap();

        assert_eq!(abi[0], 0x00);
        assert_eq!(abi[15], 0xff);
        assert_eq!(abi[31], 0x00);
        assert!(firmware_abi_from_hex("not-an-abi").is_none());
    }
}
