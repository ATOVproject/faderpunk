// Config-over-SysEx v1 codec. Mirror of libfp/src/sysex.rs — keep in sync.
//
// Envelope: F0 7D 46 50 01 <7-bit-packed payload> F7
// Packed payload (8-bit domain): u16 BE length prefix + postcard bytes.

export const SYSEX_START = 0xf0;
export const SYSEX_EOX = 0xf7;
export const SYSEX_HEADER = new Uint8Array([0x7d, 0x46, 0x50, 0x01]);

// Pack 8-bit bytes into 7-bit MIDI data bytes: per group of up to 7 input
// bytes, one MSB byte (bit i = top bit of byte i) followed by the low 7 bits
// of each byte.
export function pack7bit(src: Uint8Array): Uint8Array {
  const dst = new Uint8Array(src.length + Math.ceil(src.length / 7));
  let written = 0;
  for (let group = 0; group < src.length; group += 7) {
    const groupLen = Math.min(7, src.length - group);
    const msbIndex = written++;
    dst[msbIndex] = 0;
    for (let i = 0; i < groupLen; i++) {
      const byte = src[group + i];
      dst[msbIndex] |= (byte >> 7) << i;
      dst[written++] = byte & 0x7f;
    }
  }
  return dst;
}

export function unpack7bit(src: Uint8Array): Uint8Array {
  const dst = new Uint8Array(src.length - Math.ceil(src.length / 8));
  let written = 0;
  for (let group = 0; group < src.length; group += 8) {
    const groupLen = Math.min(8, src.length - group);
    if (groupLen < 2) {
      throw new Error("Truncated 7-bit packed data");
    }
    const msb = src[group];
    for (let i = 1; i < groupLen; i++) {
      const byte = src[group + i];
      if ((byte & 0x80) !== 0 || (msb & 0x80) !== 0) {
        throw new Error("Invalid byte in 7-bit packed data");
      }
      dst[written++] = byte | (((msb >> (i - 1)) & 1) << 7);
    }
  }
  return dst.slice(0, written);
}

// Wraps postcard bytes into a complete config SysEx frame.
export function buildConfigFrame(payload: Uint8Array): Uint8Array {
  const plain = new Uint8Array(payload.length + 2);
  plain[0] = (payload.length >> 8) & 0xff;
  plain[1] = payload.length & 0xff;
  plain.set(payload, 2);

  const packed = pack7bit(plain);
  const frame = new Uint8Array(1 + SYSEX_HEADER.length + packed.length + 1);
  frame[0] = SYSEX_START;
  frame.set(SYSEX_HEADER, 1);
  frame.set(packed, 1 + SYSEX_HEADER.length);
  frame[frame.length - 1] = SYSEX_EOX;
  return frame;
}

// Extracts the postcard bytes from a complete F0..F7 frame. Returns null for
// frames that are not ours (foreign SysEx) or fail validation.
export function parseConfigFrame(frame: Uint8Array): Uint8Array | null {
  if (
    frame.length < 2 + SYSEX_HEADER.length ||
    frame[0] !== SYSEX_START ||
    frame[frame.length - 1] !== SYSEX_EOX
  ) {
    return null;
  }
  for (let i = 0; i < SYSEX_HEADER.length; i++) {
    if (frame[1 + i] !== SYSEX_HEADER[i]) {
      return null;
    }
  }
  const packed = frame.slice(1 + SYSEX_HEADER.length, frame.length - 1);
  let plain: Uint8Array;
  try {
    plain = unpack7bit(packed);
  } catch {
    return null;
  }
  if (plain.length < 2) {
    return null;
  }
  const payloadLength = (plain[0] << 8) | plain[1];
  if (payloadLength !== plain.length - 2) {
    return null;
  }
  return plain.slice(2);
}
