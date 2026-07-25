import type { MidiEvent } from "../midi/performance";
import { SampleRing } from "./sample-ring";

export interface TrackAudioState {
  muted: boolean;
  solo: boolean;
  gain: number;
}

type TrackNodes = {
  bus: GainNode;
  /** CC monitor: shared Wave-Hz pitch, amplitude follows envelope motion. */
  liveGain: GainNode;
  liveOsc: OscillatorNode;
  liveFilter: BiquadFilterNode;
  voices: Map<number, NoteVoice>;
  ring: SampleRing;
  kind: "note" | "cc";
  muted: boolean;
  solo: boolean;
  userGain: number;
  ccEma: number;
  ccEmaInited: boolean;
  lastCcAt: number;
};

type NoteVoice = {
  osc: OscillatorNode;
  filter: BiquadFilterNode;
  g: GainNode;
  startedAt: number;
};

const MAX_NOTE_VOICES = 8;
const NOTE_ATTACK = 0.008;
const NOTE_RELEASE = 0.1;
const NOTE_AMP = 0.22;
/** CC amp from motion — idle / device-mute hold stays silent. */
const LIVE_CC_AMP = 0.14;
const LIVE_CC_SLEW = 0.03;
const LIVE_CC_EMA = 0.12;
const LIVE_MOTION_DEADBAND = 0.02;
const CC_FLAT_MS = 900;
const CC_FLAT_MIN = 0.06;
const CC_STALE_MS = 180;
const CC_WATCH_MS = 50;

/** Map Wave Hz slider (1…30) → audible carrier pitch. */
export function waveRateToCcPitchHz(waveRate: number): number {
  const t = Math.max(1, Math.min(30, waveRate));
  // ~82 Hz … ~1.05 kHz — clear, not piercing
  return 82 * 2 ** ((t - 1) / 8);
}

/**
 * Web Audio monitor:
 * - Notes → poly voices
 * - CC → sine at Wave-Hz pitch; amplitude tracks CC motion (Heat Pump ducks etc.)
 */
export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private gate: GainNode | null = null;
  private tracks = new Map<string, TrackNodes>();
  private anySolo = false;
  private masterUserGain = 0.55;
  private playing = true;
  private waveRate = 8;
  private ccWatchTimer: ReturnType<typeof setInterval> | null = null;

  async ensure(): Promise<AudioContext> {
    if (!this.ctx) {
      this.ctx = new AudioContext();
      this.gate = this.ctx.createGain();
      this.gate.gain.value = 1;
      this.master = this.ctx.createGain();
      this.master.gain.value = this.masterUserGain;
      this.gate.connect(this.master);
      this.master.connect(this.ctx.destination);
    }
    if (this.ctx.state === "suspended") await this.ctx.resume();
    this.ensureCcWatch();
    return this.ctx;
  }

  private ensureCcWatch() {
    if (this.ccWatchTimer) return;
    this.ccWatchTimer = setInterval(() => this.tickCcIdle(), CC_WATCH_MS);
  }

  private tickCcIdle() {
    if (!this.ctx || !this.playing) return;
    const nowMs = performance.now();
    const now = this.ctx.currentTime;
    for (const t of this.tracks.values()) {
      if (t.kind !== "cc" || t.muted) continue;
      const stale = t.lastCcAt > 0 && nowMs - t.lastCcAt > CC_STALE_MS;
      const flat = this.recentMotion(t.ring, CC_FLAT_MS) < CC_FLAT_MIN;
      if (stale || flat) {
        t.liveGain.gain.setTargetAtTime(0, now, 0.03);
        if (stale && t.ccEmaInited) t.ccEma = t.ring.latest;
      }
    }
  }

  isPlaying() {
    return this.playing;
  }

  setPlaying(on: boolean) {
    this.playing = on;
    if (!this.ctx || !this.gate) return;
    const now = this.ctx.currentTime;
    this.gate.gain.cancelScheduledValues(now);
    this.gate.gain.setValueAtTime(on ? 1 : 0, now);
    if (on && this.ctx.state === "suspended") void this.ctx.resume();
  }

  togglePlaying(): boolean {
    this.setPlaying(!this.playing);
    return this.playing;
  }

  panic() {
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    for (const t of this.tracks.values()) {
      for (const [note, voice] of [...t.voices.entries()]) {
        this.killVoice(t, note, voice, now);
      }
      t.liveGain.gain.cancelScheduledValues(now);
      t.liveGain.gain.setValueAtTime(0, now);
    }
    if (this.gate) {
      this.gate.gain.cancelScheduledValues(now);
      this.gate.gain.setValueAtTime(0, now);
      this.playing = false;
    }
  }

  setMasterGain(v: number) {
    this.masterUserGain = Math.max(0, Math.min(1, v));
    if (this.master) this.master.gain.value = this.masterUserGain;
  }

  setWaveRate(hz: number) {
    this.waveRate = Math.max(1, Math.min(30, hz));
    this.applyCcPitch();
  }

  private ccPitchHz(): number {
    return waveRateToCcPitchHz(this.waveRate);
  }

  private applyCcPitch() {
    if (!this.ctx) return;
    const f = this.ccPitchHz();
    const now = this.ctx.currentTime;
    for (const t of this.tracks.values()) {
      if (t.kind !== "cc") continue;
      t.liveOsc.frequency.setTargetAtTime(f, now, 0.02);
    }
  }

  registerTrack(id: string, kind: "note" | "cc", ring: SampleRing) {
    if (!this.ctx || !this.gate) return;
    const existing = this.tracks.get(id);
    if (existing) {
      existing.kind = kind;
      existing.ring = ring;
      return;
    }

    const bus = this.ctx.createGain();
    bus.gain.value = 1;
    bus.connect(this.gate);

    const liveGain = this.ctx.createGain();
    liveGain.gain.value = 0;
    const liveFilter = this.ctx.createBiquadFilter();
    liveFilter.type = "lowpass";
    liveFilter.frequency.value = 2400;
    liveFilter.Q.value = 0.4;
    const liveOsc = this.ctx.createOscillator();
    liveOsc.type = "sine";
    liveOsc.frequency.value = this.ccPitchHz();
    liveOsc.connect(liveFilter);
    liveFilter.connect(liveGain);
    liveGain.connect(bus);
    liveOsc.start();

    this.tracks.set(id, {
      bus,
      liveGain,
      liveOsc,
      liveFilter,
      voices: new Map(),
      ring,
      kind,
      muted: false,
      solo: false,
      userGain: 0.8,
      ccEma: 0,
      ccEmaInited: false,
      lastCcAt: 0,
    });
    this.applyGains();
  }

  unregisterAll() {
    if (this.ccWatchTimer) {
      clearInterval(this.ccWatchTimer);
      this.ccWatchTimer = null;
    }
    for (const track of this.tracks.values()) {
      try {
        track.liveOsc.stop();
        for (const v of track.voices.values()) {
          v.osc.stop();
          v.g.disconnect();
        }
        track.bus.disconnect();
      } catch {
        /* already stopped */
      }
    }
    this.tracks.clear();
  }

  setTrackState(id: string, state: Partial<TrackAudioState>) {
    const t = this.tracks.get(id);
    if (!t) return;
    if (state.muted !== undefined) t.muted = state.muted;
    if (state.solo !== undefined) t.solo = state.solo;
    if (state.gain !== undefined) t.userGain = state.gain;
    this.applyGains();
    if (t.muted && this.ctx) {
      const now = this.ctx.currentTime;
      t.liveGain.gain.cancelScheduledValues(now);
      t.liveGain.gain.setValueAtTime(0, now);
      for (const [note, voice] of [...t.voices.entries()]) {
        this.killVoice(t, note, voice, now);
      }
    }
  }

  private applyGains() {
    this.anySolo = [...this.tracks.values()].some((x) => x.solo);
    for (const t of this.tracks.values()) {
      const audible = !t.muted && (!this.anySolo || t.solo);
      t.bus.gain.value = audible ? t.userGain : 0;
    }
  }

  handle(id: string, ev: MidiEvent, recordToRing = true) {
    if (!this.ctx || !this.gate) return;
    if (this.ctx.state === "suspended") void this.ctx.resume();

    const t = this.tracks.get(id);
    if (!t) return;

    if (ev.kind === "cc" || ev.kind === "nrpn") {
      const v = ev.value ?? 0;
      if (recordToRing) t.ring.push(v, ev.t);
      t.lastCcAt = performance.now();
      if (!this.playing || t.muted) return;
      const now = this.ctx.currentTime;
      if (!t.ccEmaInited) {
        t.ccEma = v;
        t.ccEmaInited = true;
      } else {
        t.ccEma = t.ccEma + (v - t.ccEma) * LIVE_CC_EMA;
      }
      const rawMotion = Math.abs(v - t.ccEma);
      if (rawMotion < LIVE_MOTION_DEADBAND) {
        t.liveGain.gain.cancelScheduledValues(now);
        t.liveGain.gain.setTargetAtTime(0, now, 0.02);
      } else {
        const motion = Math.min(1, (rawMotion - LIVE_MOTION_DEADBAND) * 5);
        t.liveGain.gain.setTargetAtTime(motion * LIVE_CC_AMP, now, LIVE_CC_SLEW);
      }
      return;
    }

    if (ev.kind === "noteOn" && ev.note !== undefined) {
      if (recordToRing) t.ring.push(ev.value ?? 0.8, ev.t);
      if (this.playing && !t.muted) this.noteOn(t, ev.note, ev.velocity ?? 100);
      return;
    }
    if (ev.kind === "noteOff" && ev.note !== undefined) {
      if (recordToRing) t.ring.push(0, ev.t);
      if (this.playing) this.noteOff(t, ev.note);
    }
  }

  private noteOn(t: TrackNodes, note: number, velocity: number) {
    if (!this.ctx) return;
    this.noteOff(t, note, 0.02);
    while (t.voices.size >= MAX_NOTE_VOICES) {
      let oldestNote = -1;
      let oldestAt = Infinity;
      for (const [n, v] of t.voices) {
        if (v.startedAt < oldestAt) {
          oldestAt = v.startedAt;
          oldestNote = n;
        }
      }
      if (oldestNote < 0) break;
      this.noteOff(t, oldestNote, 0.03);
    }

    const osc = this.ctx.createOscillator();
    osc.type = "triangle";
    osc.frequency.value = 440 * 2 ** ((note - 69) / 12);

    const filter = this.ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.value = 2200 + (velocity / 127) * 1200;
    filter.Q.value = 0.3;

    const g = this.ctx.createGain();
    g.gain.value = 0;
    osc.connect(filter);
    filter.connect(g);
    g.connect(t.bus);

    const now = this.ctx.currentTime;
    const amp = Math.max(0.02, (velocity / 127) * NOTE_AMP);
    g.gain.setValueAtTime(0, now);
    g.gain.linearRampToValueAtTime(amp, now + NOTE_ATTACK);
    osc.start(now);
    t.voices.set(note, { osc, filter, g, startedAt: now });
  }

  private noteOff(t: TrackNodes, note: number, release = NOTE_RELEASE) {
    if (!this.ctx) return;
    const voice = t.voices.get(note);
    if (!voice) return;
    const now = this.ctx.currentTime;
    voice.g.gain.cancelScheduledValues(now);
    const cur = Math.max(0, voice.g.gain.value);
    voice.g.gain.setValueAtTime(cur, now);
    voice.g.gain.linearRampToValueAtTime(0, now + release);
    try {
      voice.osc.stop(now + release + 0.02);
    } catch {
      /* already stopped */
    }
    t.voices.delete(note);
  }

  private killVoice(t: TrackNodes, note: number, voice: NoteVoice, now: number) {
    try {
      voice.g.gain.cancelScheduledValues(now);
      voice.g.gain.setValueAtTime(0, now);
      voice.osc.stop(now);
    } catch {
      /* already stopped */
    }
    t.voices.delete(note);
  }

  private recentMotion(ring: SampleRing, windowMs: number): number {
    const tmp = new Float32Array(256);
    const n = ring.resampleWindow(tmp, windowMs);
    if (n < 4) return 0;
    let min = 1;
    let max = 0;
    for (let i = 0; i < n; i++) {
      min = Math.min(min, tmp[i]);
      max = Math.max(max, tmp[i]);
    }
    return max - min;
  }
}

export const audioEngine = new AudioEngine();
