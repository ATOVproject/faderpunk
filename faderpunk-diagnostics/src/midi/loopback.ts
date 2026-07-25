/**
 * Forward host-received performance MIDI back to the device USB-IN.
 * Lets apps with MidiIn hear what other apps mirror on USB-Out.
 * There is no on-device USB loopback — this is the host bridge.
 *
 * Sends to every provided output. Notes on the config cable (1) are ignored
 * by firmware; the performance cable (0) must be among the ports.
 */

export function shouldEchoToDevice(data: Uint8Array): boolean {
  if (data.length === 0) return false;
  const status = data[0];
  // Never bounce SysEx (config cable) or system realtime (clock/transport)
  if (status >= 0xf0) return false;
  // Channel voice / mode messages only (note, CC, PC, pressure, bend)
  return status >= 0x80 && status <= 0xef;
}

export function echoMidiToDevice(outputs: MIDIOutput[], data: Uint8Array): void {
  if (!shouldEchoToDevice(data)) return;
  const bytes = Array.from(data);
  // Dedupe by port id — connect may list config + performance
  const seen = new Set<string>();
  for (const output of outputs) {
    if (seen.has(output.id)) continue;
    seen.add(output.id);
    try {
      output.send(bytes);
    } catch (err) {
      console.warn("MIDI loopback send failed:", err);
    }
  }
}
