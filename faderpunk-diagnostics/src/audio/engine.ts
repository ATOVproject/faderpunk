import type { MidiEvent } from "../midi/performance";
import { SampleRing } from "./sample-ring";
import { midiToHz } from "./music";

export interface TrackAudioState {
  muted: boolean;
  solo: boolean;
  gain: number;
}

export type CcLaneSpec = {
  key: string;
  ring: SampleRing;
  ccMidi: number;
};

type CcLaneNodes = {
  liveGain: GainNode;
  liveOsc: OscillatorNode;
  liveFilter: BiquadFilterNode;
  ring: SampleRing;
  ccMidi: number;
  ccEma: number;
  ccEmaInited: boolean;
  lastCcAt: number;
};

type TrackNodes = {
  bus: GainNode;
  /** One CC carrier per MIDI out lane (separate pitches when multi-out). */
  ccLanes: Map<string, CcLaneNodes>;
  voices: Map<number, NoteVoice>;
  kind: "note" | "cc" | "hybrid";
  muted: boolean;
  solo: boolean;
  userGain: number;
};

type NoteVoice = {
  osc: OscillatorNode;
  filter: BiquadFilterNode;
  g: GainNode;
  startedAt: number;
};

const MAX_NOTE_VOICES = 12;
/** Melodic / chord apps (Vamp, Arp, sequencers). */
const NOTE_ATTACK = 0.012;
const NOTE_RELEASE = 0.14;
/** Short gate for drum-like low hits (Grooves / Grids / hybrid). */
const NOTE_ATTACK_PERC = 0.003;
const NOTE_RELEASE_PERC = 0.045;
const NOTE_AMP = 0.18;
const NOTE_AMP_PERC = 0.26;
/** CC amp from motion — idle / device-mute hold stays silent. */
const LIVE_CC_AMP = 0.14;
/** Quieter CC voice when notes are also playing (hybrid). */
const LIVE_CC_AMP_HYBRID = 0.06;
const LIVE_CC_SLEW = 0.03;
const LIVE_CC_EMA = 0.12;
const LIVE_MOTION_DEADBAND = 0.02;
const CC_FLAT_MS = 900;
const CC_FLAT_MIN = 0.06;
const CC_STALE_MS = 180;
const CC_WATCH_MS = 50;

/**
 * Web Audio monitor:
 * - Notes → poly voices at MIDI pitch (setup notes on the wire)
 * - CC → sine per out-lane key-note; amplitude tracks CC motion
 * - Hybrid → both (notes primary; quiet CC envelope under them)
 */
export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private gate: GainNode | null = null;
  private tracks = new Map<string, TrackNodes>();
  private anySolo = false;
  private masterUserGain = 0.55;
  private playing = true;
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
      if ((t.kind !== "cc" && t.kind !== "hybrid") || t.muted) continue;
      for (const lane of t.ccLanes.values()) {
        const stale = lane.lastCcAt > 0 && nowMs - lane.lastCcAt > CC_STALE_MS;
        const flat = this.recentMotion(lane.ring, CC_FLAT_MS) < CC_FLAT_MIN;
        if (stale || flat) {
          lane.liveGain.gain.setTargetAtTime(0, now, 0.03);
          if (stale && lane.ccEmaInited) lane.ccEma = lane.ring.latest;
        }
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
      t.muted = true;
      for (const [note, voice] of [...t.voices.entries()]) {
        this.killVoice(t, note, voice, now);
      }
      for (const lane of t.ccLanes.values()) {
        lane.liveGain.gain.cancelScheduledValues(now);
        lane.liveGain.gain.setValueAtTime(0, now);
      }
    }
    this.applyGains();
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

  /** Set one out-lane CC carrier to a MIDI note. */
  setLaneCcMidi(trackId: string, laneKey: string, midi: number) {
    const t = this.tracks.get(trackId);
    const lane = t?.ccLanes.get(laneKey);
    if (!t || !lane || !this.ctx) return;
    if (t.kind !== "cc" && t.kind !== "hybrid") return;
    lane.ccMidi = Math.max(0, Math.min(127, Math.round(midi)));
    lane.liveOsc.frequency.setTargetAtTime(midiToHz(lane.ccMidi), this.ctx.currentTime, 0.02);
  }

  private makeCcLane(bus: GainNode, spec: CcLaneSpec): CcLaneNodes {
    const liveGain = this.ctx!.createGain();
    liveGain.gain.value = 0;
    const liveFilter = this.ctx!.createBiquadFilter();
    liveFilter.type = "lowpass";
    liveFilter.frequency.value = 2400;
    liveFilter.Q.value = 0.4;
    const liveOsc = this.ctx!.createOscillator();
    liveOsc.type = "sine";
    liveOsc.frequency.value = midiToHz(spec.ccMidi);
    liveOsc.connect(liveFilter);
    liveFilter.connect(liveGain);
    liveGain.connect(bus);
    liveOsc.start();
    return {
      liveGain,
      liveOsc,
      liveFilter,
      ring: spec.ring,
      ccMidi: Math.max(0, Math.min(127, Math.round(spec.ccMidi))),
      ccEma: 0,
      ccEmaInited: false,
      lastCcAt: 0,
    };
  }

  private stopCcLane(lane: CcLaneNodes) {
    try {
      lane.liveOsc.stop();
      lane.liveGain.disconnect();
    } catch {
      /* already stopped */
    }
  }

  /** Sync CC out-lanes (create/update/remove carriers). */
  registerTrack(
    id: string,
    kind: "note" | "cc" | "hybrid",
    outs: CcLaneSpec[],
  ) {
    if (!this.ctx || !this.gate) return;
    const wantCc = kind === "cc" || kind === "hybrid";
    const existing = this.tracks.get(id);

    if (existing) {
      existing.kind = kind;
      if (!wantCc) {
        for (const lane of existing.ccLanes.values()) this.stopCcLane(lane);
        existing.ccLanes.clear();
        return;
      }
      const keep = new Set(outs.map((o) => o.key));
      for (const [key, lane] of [...existing.ccLanes.entries()]) {
        if (!keep.has(key)) {
          this.stopCcLane(lane);
          existing.ccLanes.delete(key);
        }
      }
      for (const spec of outs) {
        const cur = existing.ccLanes.get(spec.key);
        if (cur) {
          cur.ring = spec.ring;
          cur.ccMidi = Math.max(0, Math.min(127, Math.round(spec.ccMidi)));
          cur.liveOsc.frequency.setTargetAtTime(
            midiToHz(cur.ccMidi),
            this.ctx.currentTime,
            0.02,
          );
        } else {
          existing.ccLanes.set(spec.key, this.makeCcLane(existing.bus, spec));
        }
      }
      return;
    }

    const bus = this.ctx.createGain();
    bus.gain.value = 1;
    bus.connect(this.gate);

    const ccLanes = new Map<string, CcLaneNodes>();
    if (wantCc) {
      for (const spec of outs) {
        ccLanes.set(spec.key, this.makeCcLane(bus, spec));
      }
    }

    this.tracks.set(id, {
      bus,
      ccLanes,
      voices: new Map(),
      kind,
      muted: false,
      solo: false,
      userGain: 0.8,
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
        for (const lane of track.ccLanes.values()) this.stopCcLane(lane);
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
      for (const lane of t.ccLanes.values()) {
        lane.liveGain.gain.cancelScheduledValues(now);
        lane.liveGain.gain.setValueAtTime(0, now);
      }
      for (const [note, voice] of [...t.voices.entries()]) {
        this.killVoice(t, note, voice, now);
      }
    }
  }

  private applyGains() {
    this.anySolo = [...this.tracks.values()].some((x) => x.solo);
    if (!this.ctx) {
      for (const t of this.tracks.values()) {
        const audible = !t.muted && (!this.anySolo || t.solo);
        t.bus.gain.value = audible ? t.userGain : 0;
      }
      return;
    }
    const now = this.ctx.currentTime;
    for (const t of this.tracks.values()) {
      const audible = !t.muted && (!this.anySolo || t.solo);
      t.bus.gain.cancelScheduledValues(now);
      t.bus.gain.setValueAtTime(audible ? t.userGain : 0, now);
    }
  }

  /** Hard silence every track and mark muted in the engine. */
  muteAll() {
    for (const t of this.tracks.values()) t.muted = true;
    this.applyGains();
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    for (const t of this.tracks.values()) {
      for (const lane of t.ccLanes.values()) {
        lane.liveGain.gain.cancelScheduledValues(now);
        lane.liveGain.gain.setValueAtTime(0, now);
      }
      for (const [note, voice] of [...t.voices.entries()]) {
        this.killVoice(t, note, voice, now);
      }
    }
  }

  handle(id: string, ev: MidiEvent, recordToRing = true, laneKey?: string) {
    if (!this.ctx || !this.gate) return;
    if (this.ctx.state === "suspended") void this.ctx.resume();

    const t = this.tracks.get(id);
    if (!t) return;

    if (ev.kind === "cc" || ev.kind === "nrpn") {
      const lane =
        (laneKey ? t.ccLanes.get(laneKey) : undefined) ??
        t.ccLanes.values().next().value;
      if (!lane) return;
      const v = ev.value ?? 0;
      if (recordToRing) lane.ring.push(v, ev.t);
      lane.lastCcAt = performance.now();
      if (!this.playing || t.muted) return;
      if (t.kind !== "cc" && t.kind !== "hybrid") return;
      const now = this.ctx.currentTime;
      if (!lane.ccEmaInited) {
        lane.ccEma = v;
        lane.ccEmaInited = true;
      } else {
        lane.ccEma = lane.ccEma + (v - lane.ccEma) * LIVE_CC_EMA;
      }
      const rawMotion = Math.abs(v - lane.ccEma);
      const ampScale = t.kind === "hybrid" ? LIVE_CC_AMP_HYBRID : LIVE_CC_AMP;
      if (rawMotion < LIVE_MOTION_DEADBAND) {
        lane.liveGain.gain.cancelScheduledValues(now);
        lane.liveGain.gain.setTargetAtTime(0, now, 0.02);
      } else {
        const motion = Math.min(1, (rawMotion - LIVE_MOTION_DEADBAND) * 5);
        lane.liveGain.gain.setTargetAtTime(motion * ampScale, now, LIVE_CC_SLEW);
      }
      return;
    }

    if (ev.kind === "noteOn" && ev.note !== undefined) {
      const ring =
        (laneKey ? t.ccLanes.get(laneKey)?.ring : undefined) ??
        t.ccLanes.values().next().value?.ring;
      if (recordToRing && ring) ring.push(ev.value ?? 0.8, ev.t);
      // Always voice notes when routed here — attribution is the filter.
      if (this.playing && !t.muted) this.noteOn(t, ev.note, ev.velocity ?? 100);
      return;
    }
    if (ev.kind === "noteOff" && ev.note !== undefined) {
      const ring =
        (laneKey ? t.ccLanes.get(laneKey)?.ring : undefined) ??
        t.ccLanes.values().next().value?.ring;
      if (recordToRing && ring) ring.push(0, ev.t);
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

    // Pure note apps (Vamp/Arp/seq): sine + soft env — chords stay clean.
    // Hybrid / CC tracks: punchier low hits for drums.
    const melodic = t.kind === "note";
    const perc = !melodic && note < 48;

    const osc = this.ctx.createOscillator();
    osc.type = melodic ? "sine" : perc ? "square" : "triangle";
    osc.frequency.value = midiToHz(note);

    const filter = this.ctx.createBiquadFilter();
    filter.type = "lowpass";
    if (melodic) {
      filter.frequency.value = 1800 + (velocity / 127) * 1400;
      filter.Q.value = 0.2;
    } else if (perc) {
      filter.frequency.value = 800 + (velocity / 127) * 1800;
      filter.Q.value = 0.8;
    } else {
      filter.frequency.value = 2200 + (velocity / 127) * 1200;
      filter.Q.value = 0.3;
    }

    const g = this.ctx.createGain();
    g.gain.value = 0;
    osc.connect(filter);
    filter.connect(g);
    g.connect(t.bus);

    const now = this.ctx.currentTime;
    const attack = melodic ? NOTE_ATTACK : perc ? NOTE_ATTACK_PERC : NOTE_ATTACK;
    // 1/√n headroom so triad/7th/9th stacks don't clip into grit
    const poly = 1 / Math.sqrt(t.voices.size + 1);
    const baseAmp = melodic || !perc ? NOTE_AMP : NOTE_AMP_PERC;
    const amp = Math.max(0.02, (velocity / 127) * baseAmp * poly);
    g.gain.setValueAtTime(0, now);
    g.gain.linearRampToValueAtTime(amp, now + attack);
    osc.start(now);
    t.voices.set(note, { osc, filter, g, startedAt: now });
  }

  private noteOff(t: TrackNodes, note: number, release?: number) {
    if (!this.ctx) return;
    const voice = t.voices.get(note);
    if (!voice) return;
    const now = this.ctx.currentTime;
    const melodic = t.kind === "note";
    const rel =
      release ??
      (melodic ? NOTE_RELEASE : note < 48 ? NOTE_RELEASE_PERC : NOTE_RELEASE);
    voice.g.gain.cancelScheduledValues(now);
    const cur = Math.max(0, voice.g.gain.value);
    voice.g.gain.setValueAtTime(cur, now);
    voice.g.gain.linearRampToValueAtTime(0, now + rel);
    try {
      voice.osc.stop(now + rel + 0.03);
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
