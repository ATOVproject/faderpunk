/** MIDI System Realtime transport (cable 0 / performance). */
export function sendMidiTransport(
  outputs: MIDIOutput[],
  cmd: "start" | "stop" | "continue",
): void {
  const status = cmd === "start" ? 0xfa : cmd === "continue" ? 0xfb : 0xfc;
  for (const output of outputs) {
    try {
      output.send([status]);
    } catch (err) {
      console.warn("MIDI transport send failed:", err);
    }
  }
}
