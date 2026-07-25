/** Send classic MIDI panic on all channels via the given outputs. */
export function sendMidiPanic(outputs: MIDIOutput[]): void {
  for (const output of outputs) {
    try {
      for (let ch = 0; ch < 16; ch++) {
        const status = 0xb0 | ch;
        // CC 123 All Notes Off, CC 120 All Sound Off, CC 121 Reset All Controllers
        output.send([status, 123, 0]);
        output.send([status, 120, 0]);
        output.send([status, 121, 0]);
      }
    } catch (err) {
      console.warn("MIDI panic send failed:", err);
    }
  }
}
