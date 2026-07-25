/** Ring buffer of timed samples for scope + profile. */
export class SampleRing {
  readonly capacity: number;
  private values: Float32Array;
  private times: Float64Array;
  private write = 0;
  private filled = 0;
  latest = 0;
  lastT = 0;

  constructor(capacity = 2048) {
    this.capacity = capacity;
    this.values = new Float32Array(capacity);
    this.times = new Float64Array(capacity);
  }

  push(value: number, t = performance.now()) {
    const v = Math.max(0, Math.min(1, value));
    this.values[this.write] = v;
    this.times[this.write] = t;
    this.write = (this.write + 1) % this.capacity;
    if (this.filled < this.capacity) this.filled++;
    this.latest = v;
    this.lastT = t;
  }

  get length() {
    return this.filled;
  }

  /** Oldest → newest into parallel buffers. Returns count. */
  copyChronological(outValues: Float32Array, outTimes?: Float64Array): number {
    const n = Math.min(outValues.length, this.filled);
    const start = (this.write - n + this.capacity) % this.capacity;
    for (let i = 0; i < n; i++) {
      const idx = (start + i) % this.capacity;
      outValues[i] = this.values[idx];
      if (outTimes) outTimes[i] = this.times[idx];
    }
    return n;
  }

  /**
   * Resample to a fixed window ending at `now` (ms), sample-and-hold between events.
   * X axis = linear wall-clock time. Gaps stay gaps.
   */
  resampleWindow(out: Float32Array, windowMs: number, now = performance.now()): number {
    const bins = out.length;
    out.fill(0);
    if (this.filled === 0 || bins < 2) return 0;

    const t0 = now - windowMs;
    const start = (this.write - this.filled + this.capacity) % this.capacity;

    // Value held at start of window (last sample before t0, else 0)
    let held = 0;
    let i = 0;
    for (; i < this.filled; i++) {
      const idx = (start + i) % this.capacity;
      if (this.times[idx] >= t0) break;
      held = this.values[idx];
    }

    let ev = i;
    for (let b = 0; b < bins; b++) {
      const t = t0 + (b / (bins - 1)) * windowMs;
      while (ev < this.filled) {
        const idx = (start + ev) % this.capacity;
        if (this.times[idx] > t) break;
        held = this.values[idx];
        ev++;
      }
      out[b] = held;
    }
    return bins;
  }

  /** Mean waveform profile: fold samples into `bins` phase buckets via zero-crossings. */
  profile(bins = 128): Float32Array {
    const tmp = new Float32Array(this.capacity);
    const n = this.copyChronological(tmp);
    const out = new Float32Array(bins);
    const counts = new Float32Array(bins);
    if (n < 8) return out;

    const mid = 0.5;
    const cycles: number[] = [];
    for (let i = 1; i < n; i++) {
      if (tmp[i - 1] < mid && tmp[i] >= mid) cycles.push(i);
    }
    if (cycles.length < 2) {
      for (let b = 0; b < bins; b++) {
        const idx = Math.floor((b / bins) * (n - 1));
        out[b] = tmp[idx];
        counts[b] = 1;
      }
      return out;
    }

    for (let c = 0; c < cycles.length - 1; c++) {
      const a = cycles[c];
      const b = cycles[c + 1];
      const len = b - a;
      if (len < 4) continue;
      for (let i = 0; i < len; i++) {
        const bin = Math.min(bins - 1, Math.floor((i / len) * bins));
        out[bin] += tmp[a + i];
        counts[bin]++;
      }
    }
    for (let b = 0; b < bins; b++) {
      if (counts[b] > 0) out[b] /= counts[b];
    }
    return out;
  }

  clear() {
    this.write = 0;
    this.filled = 0;
    this.latest = 0;
    this.values.fill(0);
    this.times.fill(0);
  }
}
