/** Ring buffer of normalized samples for scope + profile. */
export class SampleRing {
  readonly capacity: number;
  private buf: Float32Array;
  private write = 0;
  private filled = 0;
  latest = 0;
  lastT = 0;

  constructor(capacity = 2048) {
    this.capacity = capacity;
    this.buf = new Float32Array(capacity);
  }

  push(value: number, t = performance.now()) {
    const v = Math.max(0, Math.min(1, value));
    this.buf[this.write] = v;
    this.write = (this.write + 1) % this.capacity;
    if (this.filled < this.capacity) this.filled++;
    this.latest = v;
    this.lastT = t;
  }

  /** Oldest → newest copy into `out` (length = filled). */
  copyChronological(out: Float32Array): number {
    const n = Math.min(out.length, this.filled);
    const start = (this.write - n + this.capacity) % this.capacity;
    for (let i = 0; i < n; i++) {
      out[i] = this.buf[(start + i) % this.capacity];
    }
    return n;
  }

  /** Mean waveform profile: fold samples into `bins` phase buckets via zero-crossings. */
  profile(bins = 128): Float32Array {
    const tmp = new Float32Array(this.capacity);
    const n = this.copyChronological(tmp);
    const out = new Float32Array(bins);
    const counts = new Float32Array(bins);
    if (n < 8) return out;

    // Detect rising edges around mid as cycle starts
    const mid = 0.5;
    const cycles: number[] = [];
    for (let i = 1; i < n; i++) {
      if (tmp[i - 1] < mid && tmp[i] >= mid) cycles.push(i);
    }
    if (cycles.length < 2) {
      // Fallback: linear stretch of whole buffer
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
    this.buf.fill(0);
  }
}
