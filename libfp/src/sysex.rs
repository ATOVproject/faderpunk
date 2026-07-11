//! Config-over-SysEx v1 wire format shared between firmware and configurator.
//!
//! Envelope: `F0 7D 46 50 01 <7-bit-packed payload> F7`
//!
//! The packed payload (in the 8-bit domain, before packing) is a u16 BE length
//! prefix followed by the postcard-serialized `ConfigMsgIn`/`ConfigMsgOut`.
//! The configurator mirrors this codec in `configurator/src/utils/sysex.ts` —
//! keep both sides in sync.

pub const SYSEX_START: u8 = 0xF0;
pub const SYSEX_EOX: u8 = 0xF7;

/// Bytes between `F0` and the packed payload: manufacturer ID 0x7D
/// (non-commercial), "FP" device signature, protocol version 1. A registered
/// manufacturer ID can replace 0x7D later by changing only this constant (and
/// its TS mirror).
pub const SYSEX_HEADER: [u8; 4] = [0x7D, 0x46, 0x50, 0x01];

/// Max postcard payload size (unchanged from the WebUSB protocol).
pub const MAX_PAYLOAD_SIZE: usize = 512;
/// Payload plus the u16 BE length prefix.
pub const MAX_PLAIN_SIZE: usize = MAX_PAYLOAD_SIZE + 2;
/// `packed_len(MAX_PLAIN_SIZE)`.
pub const MAX_PACKED_SIZE: usize = packed_len(MAX_PLAIN_SIZE);
/// Full frame: F0 + header + packed payload + F7.
pub const MAX_SYSEX_FRAME: usize = 1 + SYSEX_HEADER.len() + MAX_PACKED_SIZE + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysexError {
    /// A packed byte had its top bit set.
    InvalidByte,
    /// Destination buffer too small.
    BufferTooSmall,
    /// Packed input ended in the middle of a group in an invalid way.
    Truncated,
}

/// Packed size for `n` plain bytes: one MSB byte per group of up to 7.
pub const fn packed_len(n: usize) -> usize {
    n + n.div_ceil(7)
}

/// Pack 8-bit bytes into 7-bit MIDI data bytes. For each group of up to 7
/// input bytes, emits one MSB byte (bit i = top bit of byte i) followed by the
/// low 7 bits of each byte. Returns the number of bytes written.
pub fn pack_7bit(src: &[u8], dst: &mut [u8]) -> Result<usize, SysexError> {
    if dst.len() < packed_len(src.len()) {
        return Err(SysexError::BufferTooSmall);
    }
    let mut written = 0;
    for group in src.chunks(7) {
        let msb_idx = written;
        dst[msb_idx] = 0;
        written += 1;
        for (i, &byte) in group.iter().enumerate() {
            dst[msb_idx] |= (byte >> 7) << i;
            dst[written] = byte & 0x7F;
            written += 1;
        }
    }
    Ok(written)
}

/// Inverse of [`pack_7bit`]. Returns the number of plain bytes written.
pub fn unpack_7bit(src: &[u8], dst: &mut [u8]) -> Result<usize, SysexError> {
    let mut written = 0;
    for group in src.chunks(8) {
        // A group must contain the MSB byte plus at least one data byte.
        if group.len() < 2 {
            return Err(SysexError::Truncated);
        }
        let msb = group[0];
        for (i, &byte) in group[1..].iter().enumerate() {
            if byte & 0x80 != 0 || msb & 0x80 != 0 {
                return Err(SysexError::InvalidByte);
            }
            if written >= dst.len() {
                return Err(SysexError::BufferTooSmall);
            }
            dst[written] = byte | (((msb >> i) & 1) << 7);
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(plain: &[u8]) {
        let mut packed = [0u8; packed_len(MAX_PLAIN_SIZE)];
        let packed_size = pack_7bit(plain, &mut packed).unwrap();
        assert_eq!(packed_size, packed_len(plain.len()));
        assert!(packed[..packed_size].iter().all(|b| b & 0x80 == 0));
        let mut unpacked = [0u8; MAX_PLAIN_SIZE];
        let plain_size = unpack_7bit(&packed[..packed_size], &mut unpacked).unwrap();
        assert_eq!(&unpacked[..plain_size], plain);
    }

    #[test]
    fn empty_roundtrip() {
        roundtrip(&[]);
    }

    #[test]
    fn small_roundtrips() {
        roundtrip(&[0x00]);
        roundtrip(&[0xFF]);
        roundtrip(&[0x80, 0x7F, 0x00, 0xFF, 0x01, 0xFE, 0xAA]);
        roundtrip(&[0x80, 0x7F, 0x00, 0xFF, 0x01, 0xFE, 0xAA, 0x55]);
    }

    #[test]
    fn max_size_roundtrip() {
        let mut plain = [0u8; MAX_PLAIN_SIZE];
        for (i, byte) in plain.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }
        roundtrip(&plain);
    }

    #[test]
    fn unpack_rejects_high_bit() {
        let mut dst = [0u8; 8];
        assert_eq!(
            unpack_7bit(&[0x00, 0x80], &mut dst),
            Err(SysexError::InvalidByte)
        );
    }

    #[test]
    fn unpack_rejects_lone_msb_byte() {
        let mut dst = [0u8; 8];
        assert_eq!(unpack_7bit(&[0x01], &mut dst), Err(SysexError::Truncated));
    }

    #[test]
    fn frame_size_consts() {
        assert_eq!(MAX_PACKED_SIZE, 588);
        assert_eq!(MAX_SYSEX_FRAME, 594);
    }
}
