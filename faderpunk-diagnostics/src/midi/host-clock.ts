/** Host MIDI clock (24 PPQN) for driving the device when Clock Src = MIDI USB. */

export class HostClock {
  private timer: ReturnType<typeof setInterval> | null = null;
  private bpm = 120;
  private outputs: MIDIOutput[] = [];
  private running = false;

  isRunning() {
    return this.running;
  }

  getBpm() {
    return this.bpm;
  }

  setOutputs(outputs: MIDIOutput[]) {
    this.outputs = outputs;
  }

  setBpm(bpm: number) {
    this.bpm = Math.max(20, Math.min(300, Math.round(bpm)));
    if (this.running) this.restartTicks();
  }

  /** Send Start (0xFA) and begin clock ticks. */
  start() {
    this.send(0xfa);
    this.running = true;
    this.restartTicks();
  }

  /** Send Continue (0xFB) and ensure ticks are running. */
  continue() {
    this.send(0xfb);
    this.running = true;
    if (!this.timer) this.restartTicks();
  }

  /** Stop ticks and send Stop (0xFC). */
  stop() {
    this.clearTicks();
    this.running = false;
    this.send(0xfc);
  }

  /** Silence ticks without Stop (e.g. disconnect). */
  halt() {
    this.clearTicks();
    this.running = false;
  }

  private restartTicks() {
    this.clearTicks();
    const ms = 60_000 / (this.bpm * 24);
    this.timer = setInterval(() => this.send(0xf8), ms);
  }

  private clearTicks() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  private send(status: number) {
    for (const output of this.outputs) {
      try {
        output.send([status]);
      } catch (err) {
        console.warn("Host MIDI clock send failed:", err);
      }
    }
  }
}

export const hostClock = new HostClock();
