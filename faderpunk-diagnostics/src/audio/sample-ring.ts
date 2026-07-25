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
   * X axis = linear wall-clock time.
   *
   * If the ring overflowed and the oldest kept sample is still inside the
   * window, hold that value across the truncated left edge (do not drop to 0 —
   * that looked like the wave was "disappearing").
   * A true quiet stretch before the first event still stays at 0 (ring not full).
   */
  resampleWindow(out: Float32Array, windowMs: number, now = performance.now()): number {
    const bins = out.length;
    out.fill(0);
    if (this.filled === 0 || bins < 2) return 0;

    const t0 = now - windowMs;
    const start = (this.write - this.filled + this.capacity) % this.capacity;
    const ringFull = this.filled >= this.capacity;

    // Value held at start of window (last sample before t0).
    let held = 0;
    let i = 0;
    for (; i < this.filled; i++) {
      const idx = (start + i) % this.capacity;
      if (this.times[idx] >= t0) break;
      held = this.values[idx];
    }
    // Truncated history: oldest sample is newer than t0, but we wrapped —
    // keep holding the oldest value instead of a fake zero cliff.
    if (i === 0 && ringFull) {
      held = this.values[start];
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

  /** Peak in the last `windowMs` (for quiet detection). */
  recentPeak(windowMs = 8000, now = performance.now()): number {
    if (this.filled === 0) return 0;
    const t0 = now - windowMs;
    const start = (this.write - this.filled + this.capacity) % this.capacity;
    let peak = 0;
    for (let i = 0; i < this.filled; i++) {
      const idx = (start + i) % this.capacity;
      if (this.times[idx] < t0) continue;
      if (this.values[idx] > peak) peak = this.values[idx];
    }
    return peak;
  }

  clear() {
    this.write = 0;
    this.filled = 0;
    this.latest = 0;
    this.lastT = 0;
    this.values.fill(0);
    this.times.fill(0);
  }
}

const PROFILE_DENSE = 512;

function risingOnsets(tmp: Float32Array, mid: number): number[] {
  const out: number[] = [];
  for (let i = 1; i < tmp.length; i++) {
    if (tmp[i - 1] < mid && tmp[i] >= mid) out.push(i);
  }
  return out;
}

function medianInt(values: number[]): number {
  if (values.length === 0) return 0;
  const s = values.slice().sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)] ?? 0;
}

/**
 * One avg for every track: time-align outs → optional Σ/Π → mean pulse after each
 * rising edge (fixed length = median inter-onset). Left = attack, not wall-clock.
 */
export function avgOutProfile(
  rings: SampleRing[],
  bins = 96,
  mode: "add" | "mul" = "add",
  windowMs = 8000,
): Float32Array {
  const out = new Float32Array(bins);
  if (rings.length === 0) return out;

  const dense = new Float32Array(PROFILE_DENSE);
  const tmp = new Float32Array(PROFILE_DENSE);

  if (rings.length === 1) {
    rings[0].resampleWindow(dense, windowMs);
  } else {
    if (mode === "mul") dense.fill(1);
    for (const ring of rings) {
      ring.resampleWindow(tmp, windowMs);
      if (mode === "add") {
        for (let i = 0; i < PROFILE_DENSE; i++) {
          dense[i] = Math.min(1, dense[i] + tmp[i]);
        }
      } else {
        for (let i = 0; i < PROFILE_DENSE; i++) {
          dense[i] *= tmp[i];
        }
      }
    }
  }

  let peak = 0;
  for (let i = 0; i < PROFILE_DENSE; i++) if (dense[i] > peak) peak = dense[i];
  if (peak < 0.02) return out;

  const mid = Math.max(0.15, peak * 0.4);
  const onsets = risingOnsets(dense, mid);
  if (onsets.length === 0) {
    for (let b = 0; b < bins; b++) {
      out[b] = dense[Math.floor((b / bins) * (PROFILE_DENSE - 1))];
    }
    return out;
  }

  // Fixed slice after each onset (median IOI). Do NOT stretch each cycle to full
  // width — that made irregular gates (Golden Gate) look like noise + flat tails.
  const gaps: number[] = [];
  for (let i = 0; i < onsets.length - 1; i++) {
    gaps.push(onsets[i + 1]! - onsets[i]!);
  }
  const slice =
    gaps.length > 0
      ? Math.max(8, Math.min(PROFILE_DENSE, medianInt(gaps)))
      : Math.max(8, Math.floor(PROFILE_DENSE / 4));

  const counts = new Float32Array(bins);
  for (const start of onsets) {
    if (start + 4 >= PROFILE_DENSE) continue;
    const len = Math.min(slice, PROFILE_DENSE - start);
    for (let i = 0; i < len; i++) {
      const bin = Math.min(bins - 1, Math.floor((i / slice) * bins));
      out[bin] += dense[start + i]!;
      counts[bin]++;
    }
  }

  for (let b = 0; b < bins; b++) {
    if (counts[b] > 0) out[b] /= counts[b];
  }
  return out;
}

/** @deprecated use avgOutProfile — kept as alias for call sites. */
export function combinedOutProfile(
  rings: SampleRing[],
  bins = 96,
  mode: "add" | "mul" = "add",
  windowMs = 8000,
): Float32Array {
  return avgOutProfile(rings, bins, mode, windowMs);
}
