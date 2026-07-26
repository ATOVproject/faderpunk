// Config-over-SysEx v1 codec. Mirror of libfp/src/sysex.rs — keep in sync.
// Envelope: F0 7D 46 50 01 <7-bit-packed payload> F7

export const SYSEX_START = 0xf0;
export const SYSEX_EOX = 0xf7;
export const SYSEX_HEADER = new Uint8Array([0x7d, 0x46, 0x50, 0x01]);

export function pack7bit(src) {
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

export function unpack7bit(src) {
  const dst = new Uint8Array(src.length - Math.ceil(src.length / 8));
  let written = 0;
  for (let group = 0; group < src.length; group += 8) {
    const groupLen = Math.min(8, src.length - group);
    if (groupLen < 2) throw new Error("Truncated 7-bit packed data");
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

export function buildConfigFrame(payload) {
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

export function parseConfigFrame(frame) {
  if (
    frame.length < 2 + SYSEX_HEADER.length ||
    frame[0] !== SYSEX_START ||
    frame[frame.length - 1] !== SYSEX_EOX
  ) {
    return null;
  }
  for (let i = 0; i < SYSEX_HEADER.length; i++) {
    if (frame[1 + i] !== SYSEX_HEADER[i]) return null;
  }
  const packed = frame.slice(1 + SYSEX_HEADER.length, frame.length - 1);
  try {
    const plain = unpack7bit(packed);
    if (plain.length < 2) return null;
    const len = (plain[0] << 8) | plain[1];
    if (plain.length < 2 + len) return null;
    return plain.slice(2, 2 + len);
  } catch {
    return null;
  }
}
