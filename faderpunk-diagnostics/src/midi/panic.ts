/** Classic MIDI panic CCs on one channel (1–16). */
function panicChannel(output: MIDIOutput, channel: number): void {
  const ch = Math.max(1, Math.min(16, Math.round(channel))) - 1;
  const status = 0xb0 | ch;
  // CC 123 All Notes Off, CC 120 All Sound Off, CC 121 Reset All Controllers
  output.send([status, 123, 0]);
  output.send([status, 120, 0]);
  output.send([status, 121, 0]);
}

/** Send classic MIDI panic on all channels via the given outputs. */
export function sendMidiPanic(outputs: MIDIOutput[]): void {
  for (const output of outputs) {
    try {
      for (let ch = 1; ch <= 16; ch++) panicChannel(output, ch);
    } catch (err) {
      console.warn("MIDI panic send failed:", err);
    }
  }
}

/** Panic only the given MIDI channels (1–16) on each output. */
export function sendMidiPanicChannels(
  outputs: MIDIOutput[],
  channels: number[],
): void {
  const uniq = [...new Set(channels.map((c) => Math.max(1, Math.min(16, Math.round(c)))))];
  if (uniq.length === 0) return;
  for (const output of outputs) {
    try {
      for (const ch of uniq) panicChannel(output, ch);
    } catch (err) {
      console.warn("MIDI channel panic send failed:", err);
    }
  }
}
