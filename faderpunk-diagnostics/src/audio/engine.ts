import type { MidiEvent } from "../midi/performance";
import { SampleRing } from "./sample-ring";

export interface TrackAudioState {
  muted: boolean;
  solo: boolean;
  gain: number;
}

/**
 * Web Audio monitor:
 * - Note tracks → simple poly saw voices
 * - CC/NRPN tracks → carrier whose amplitude (and mild pitch) follows the control,
 *   plus optional wavetable playback of the captured shape ("hear the wave")
 */
export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private tracks = new Map<
    string,
    {
      bus: GainNode;
      ccGain: GainNode;
      osc: OscillatorNode;
      waveGain: GainNode;
      wave: OscillatorNode;
      waveBuf: PeriodicWave | null;
      voices: Map<number, { osc: OscillatorNode; g: GainNode }>;
      ring: SampleRing;
      kind: "note" | "cc";
      muted: boolean;
      solo: boolean;
      userGain: number;
    }
  >();
  private anySolo = false;
  waveRateHz = 8; // audible playback rate of captured profile

  async ensure(): Promise<AudioContext> {
    if (!this.ctx) {
      this.ctx = new AudioContext();
      this.master = this.ctx.createGain();
      this.master.gain.value = 0.35;
      this.master.connect(this.ctx.destination);
    }
    if (this.ctx.state === "suspended") await this.ctx.resume();
    return this.ctx;
  }

  setMasterGain(v: number) {
    if (this.master) this.master.gain.value = Math.max(0, Math.min(1, v));
  }

  registerTrack(id: string, kind: "note" | "cc", ring: SampleRing) {
    if (!this.ctx || !this.master) return;
    if (this.tracks.has(id)) return;

    const bus = this.ctx.createGain();
    bus.gain.value = 1;
    bus.connect(this.master);

    const ccGain = this.ctx.createGain();
    ccGain.gain.value = 0;
    const osc = this.ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.value = 110;
    osc.connect(ccGain);
    ccGain.connect(bus);
    osc.start();

    const waveGain = this.ctx.createGain();
    waveGain.gain.value = 0;
    const wave = this.ctx.createOscillator();
    wave.frequency.value = this.waveRateHz;
    wave.connect(waveGain);
    waveGain.connect(bus);
    wave.start();

    this.tracks.set(id, {
      bus,
      ccGain,
      osc,
      waveGain,
      wave,
      waveBuf: null,
      voices: new Map(),
      ring,
      kind,
      muted: false,
      solo: false,
      userGain: 0.8,
    });
    this.applyGains();
  }

  unregisterAll() {
    for (const track of this.tracks.values()) {
      try {
        track.osc.stop();
        track.wave.stop();
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
    this.anySolo = [...this.tracks.values()].some((x) => x.solo);
    this.applyGains();
  }

  private applyGains() {
    this.anySolo = [...this.tracks.values()].some((x) => x.solo);
    for (const t of this.tracks.values()) {
      const audible = !t.muted && (!this.anySolo || t.solo);
      t.bus.gain.value = audible ? t.userGain : 0;
    }
  }

  /** Feed a MIDI event already attributed to a track. */
  handle(id: string, ev: MidiEvent) {
    const t = this.tracks.get(id);
    if (!t || !this.ctx) return;

    if (ev.kind === "cc" || ev.kind === "nrpn") {
      const v = ev.value ?? 0;
      t.ring.push(v, ev.t);
      const now = this.ctx.currentTime;
      t.ccGain.gain.setTargetAtTime(v * 0.25, now, 0.01);
      t.osc.frequency.setTargetAtTime(55 + v * 440, now, 0.02);
      this.refreshWave(id);
      return;
    }

    if (ev.kind === "noteOn" && ev.note !== undefined) {
      t.ring.push(ev.value ?? 0.8, ev.t);
      this.noteOn(t, ev.note, ev.velocity ?? 100);
      return;
    }
    if (ev.kind === "noteOff" && ev.note !== undefined) {
      t.ring.push(0, ev.t);
      this.noteOff(t, ev.note);
    }
  }

  private noteOn(
    t: {
      bus: GainNode;
      voices: Map<number, { osc: OscillatorNode; g: GainNode }>;
    },
    note: number,
    velocity: number,
  ) {
    if (!this.ctx) return;
    this.noteOff(t, note);
    const osc = this.ctx.createOscillator();
    osc.type = "sawtooth";
    osc.frequency.value = 440 * 2 ** ((note - 69) / 12);
    const g = this.ctx.createGain();
    g.gain.value = 0;
    osc.connect(g);
    g.connect(t.bus);
    const now = this.ctx.currentTime;
    const amp = (velocity / 127) * 0.2;
    g.gain.setValueAtTime(0, now);
    g.gain.linearRampToValueAtTime(amp, now + 0.005);
    osc.start(now);
    t.voices.set(note, { osc, g });
  }

  private noteOff(
    t: { voices: Map<number, { osc: OscillatorNode; g: GainNode }> },
    note: number,
  ) {
    if (!this.ctx) return;
    const voice = t.voices.get(note);
    if (!voice) return;
    const now = this.ctx.currentTime;
    voice.g.gain.cancelScheduledValues(now);
    voice.g.gain.setValueAtTime(voice.g.gain.value, now);
    voice.g.gain.linearRampToValueAtTime(0, now + 0.04);
    voice.osc.stop(now + 0.05);
    t.voices.delete(note);
  }

  private refreshWave(id: string) {
    const t = this.tracks.get(id);
    if (!t || !this.ctx || t.kind !== "cc") return;
    const profile = t.ring.profile(64);
    // Need non-flat profile
    let min = 1;
    let max = 0;
    for (const s of profile) {
      min = Math.min(min, s);
      max = Math.max(max, s);
    }
    if (max - min < 0.05) {
      t.waveGain.gain.setTargetAtTime(0, this.ctx.currentTime, 0.05);
      return;
    }
    const real = new Float32Array(profile.length);
    const imag = new Float32Array(profile.length);
    for (let i = 0; i < profile.length; i++) {
      real[i] = (profile[i] - min) / (max - min) * 2 - 1;
    }
    const wave = this.ctx.createPeriodicWave(real, imag, {
      disableNormalization: false,
    });
    t.wave.setPeriodicWave(wave);
    t.wave.frequency.setTargetAtTime(this.waveRateHz, this.ctx.currentTime, 0.05);
    t.waveGain.gain.setTargetAtTime(0.12, this.ctx.currentTime, 0.05);
  }

  setWaveRate(hz: number) {
    this.waveRateHz = Math.max(0.5, Math.min(40, hz));
    for (const t of this.tracks.values()) {
      if (this.ctx) t.wave.frequency.setTargetAtTime(this.waveRateHz, this.ctx.currentTime, 0.05);
    }
  }
}

export const audioEngine = new AudioEngine();
