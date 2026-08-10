use std::io::{self, Read, Write};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const CHANNELS: usize = 16;
pub const BUTTONS: usize = 18;
pub const PORTS: usize = 20;
pub const LEDS: usize = 50;
const MAX_FRAME_LEN: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostToCore {
    Fader { channel: u8, value: u16 },
    Button { index: u8, pressed: bool },
    Adc { port: u8, value: u16 },
    TransportToggle,
    PerformanceMidi(Vec<u8>),
    ConfigMidi(Vec<u8>),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CoreToHost {
    Ready { firmware_version: (u8, u8, u8) },
    Snapshot(CoreSnapshot),
    PerformanceMidi(Vec<u8>),
    ConfigMidi(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoreSnapshot {
    pub leds: Vec<u32>,
    pub latched_faders: [u16; CHANNELS],
    pub adc: [u16; PORTS],
    pub dac: [u16; PORTS],
    pub port_modes: [u8; PORTS],
    pub port_ranges: [u8; PORTS],
    pub gates: [bool; PORTS],
    pub clock_running: bool,
    pub current_scene: Option<u8>,
    pub bpm: f32,
    pub swing: i8,
}

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let payload = postcard::to_stdvec(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "IPC frame too large"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut len = [0_u8; 4];
    if reader.read(&mut len[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut len[1..])?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame exceeds limit",
        ));
    }
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    postcard::from_bytes(&payload)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framed_messages_round_trip_in_sequence() {
        let messages = [
            HostToCore::Fader {
                channel: 3,
                value: 2048,
            },
            HostToCore::ConfigMidi(vec![0xf0, 0x7d, 0xf7]),
        ];
        let mut bytes = Vec::new();
        for message in &messages {
            write_frame(&mut bytes, message).unwrap();
        }

        let mut reader = Cursor::new(bytes);
        for expected in messages {
            assert_eq!(read_frame(&mut reader).unwrap(), Some(expected));
        }
        assert_eq!(read_frame::<HostToCore>(&mut reader).unwrap(), None);
    }

    #[test]
    fn partial_length_prefix_is_an_error() {
        let error = read_frame::<HostToCore>(&mut Cursor::new([1_u8, 0]))
            .expect_err("partial prefix must not look like clean EOF");
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocating() {
        let prefix = ((MAX_FRAME_LEN + 1) as u32).to_le_bytes();
        let error = read_frame::<HostToCore>(&mut Cursor::new(prefix))
            .expect_err("oversized frame must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
