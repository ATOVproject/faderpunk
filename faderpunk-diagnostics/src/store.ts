import { create } from "zustand";

import { audioEngine } from "./audio/engine";
import { SampleRing } from "./audio/sample-ring";
import type { AppTrack, Snapshot } from "./mapping/tracks";
import { loadSnapshot, refreshTrackParams } from "./mapping/tracks";
import { PerformanceParser, type MidiEvent } from "./midi/performance";

export type ViewMode = "all" | "solo" | "compare";

export interface TrackRuntime {
  key: string;
  track: AppTrack;
  ring: SampleRing;
  muted: boolean;
  solo: boolean;
  selected: boolean; // compare selection / focus
  activity: number; // 0–1 decay
  lastEvent: MidiEvent | null;
  unmatchedHint: string | null;
}

interface DiagState {
  status: "idle" | "connecting" | "ready" | "error";
  error: string | null;
  version: string | null;
  demo: boolean;
  viewMode: ViewMode;
  focusKey: string | null;
  masterGain: number;
  waveRate: number;
  showUnmapped: boolean;
  tracks: TrackRuntime[];
  unmappedLog: MidiEvent[];
  clockCount: number;
  connect: () => Promise<void>;
  disconnect: () => void;
  startDemo: () => void;
  setViewMode: (m: ViewMode) => void;
  setFocus: (key: string | null) => void;
  toggleMute: (key: string) => void;
  toggleSolo: (key: string) => void;
  toggleCompare: (key: string) => void;
  setMasterGain: (v: number) => void;
  setWaveRate: (v: number) => void;
  refreshParams: () => Promise<void>;
  ingest: (ev: MidiEvent) => void;
}

let snapshot: Snapshot | null = null;
let perfHandler: ((e: MIDIMessageEvent) => void) | null = null;
const parser = new PerformanceParser();
let demoTimer: ReturnType<typeof setInterval> | null = null;

function visibleKeys(state: DiagState): Set<string> | null {
  if (state.viewMode === "all") return null;
  if (state.viewMode === "solo" && state.focusKey) return new Set([state.focusKey]);
  if (state.viewMode === "compare") {
    return new Set(state.tracks.filter((t) => t.selected).map((t) => t.key));
  }
  return null;
}

function routeEvent(tracks: TrackRuntime[], ev: MidiEvent): TrackRuntime[] {
  if (ev.kind === "clock" || ev.kind === "transport" || ev.channel === 0) {
    return [];
  }

  const candidates = tracks.filter((tr) => {
    if (tr.track.midi.channel !== ev.channel) return false;
    if (ev.kind === "cc" || ev.kind === "nrpn") {
      if (tr.track.midi.noteMode && tr.track.midi.cc === null) return false;
      if (tr.track.midi.cc !== null && ev.cc !== undefined) {
        return tr.track.midi.cc === ev.cc;
      }
      return !tr.track.midi.noteMode;
    }
    if (ev.kind === "noteOn" || ev.kind === "noteOff") {
      return tr.track.midi.noteMode || tr.track.midi.cc === null;
    }
    return false;
  });

  // Prefer USB-enabled mirrors when multiple match
  const usb = candidates.filter((c) => c.track.midi.usbEnabled);
  return usb.length > 0 ? usb : candidates;
}

function stopDemo() {
  if (demoTimer) {
    clearInterval(demoTimer);
    demoTimer = null;
  }
}

function detachPerf() {
  if (snapshot?.device.performanceInput && perfHandler) {
    snapshot.device.performanceInput.onmidimessage = null;
  }
  perfHandler = null;
}

export const useDiag = create<DiagState>((set, get) => ({
  status: "idle",
  error: null,
  version: null,
  demo: false,
  viewMode: "all",
  focusKey: null,
  masterGain: 0.35,
  waveRate: 8,
  showUnmapped: true,
  tracks: [],
  unmappedLog: [],
  clockCount: 0,

  connect: async () => {
    stopDemo();
    detachPerf();
    audioEngine.unregisterAll();
    set({ status: "connecting", error: null, demo: false });
    try {
      const snap = await loadSnapshot();
      snapshot = snap;
      await audioEngine.ensure();
      audioEngine.setMasterGain(get().masterGain);
      audioEngine.setWaveRate(get().waveRate);

      const tracks: TrackRuntime[] = snap.tracks.map((track: AppTrack) => {
        const ring = new SampleRing(2048);
        const kind = track.midi.noteMode ? "note" : "cc";
        audioEngine.registerTrack(track.key, kind, ring);
        return {
          key: track.key,
          track,
          ring,
          muted: false,
          solo: false,
          selected: false,
          activity: 0,
          lastEvent: null,
          unmatchedHint: track.midi.usbEnabled
            ? null
            : track.hasMidiMirror
              ? "USB MIDI out disabled on device — enable MidiOut→USB in configurator"
              : "No MIDI mirror (CV-only app)",
        };
      });

      if (snap.device.performanceInput) {
        perfHandler = (event: MIDIMessageEvent) => {
          if (!event.data) return;
          const events = parser.parse(new Uint8Array(event.data));
          for (const ev of events) get().ingest(ev);
        };
        snap.device.performanceInput.onmidimessage = perfHandler;
      }

      set({
        status: "ready",
        version: snap.version,
        tracks,
        focusKey: tracks[0]?.key ?? null,
        unmappedLog: [],
        clockCount: 0,
      });
    } catch (err) {
      set({
        status: "error",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  disconnect: () => {
    stopDemo();
    detachPerf();
    audioEngine.unregisterAll();
    snapshot = null;
    set({
      status: "idle",
      version: null,
      tracks: [],
      demo: false,
      unmappedLog: [],
    });
  },

  startDemo: () => {
    stopDemo();
    detachPerf();
    audioEngine.unregisterAll();
    void audioEngine.ensure().then(() => {
      audioEngine.setMasterGain(get().masterGain);
      const fakeTracks: AppTrack[] = [
        {
          key: "demo-lfo",
          layoutId: 0,
          startChannel: 0,
          width: 1,
          app: {
            appId: 2,
            channels: 1,
            name: "LFO (demo)",
            description: "Synthetic CC stream",
            color: "Cyan",
            params: [],
          },
          midi: { usbEnabled: true, channel: 1, cc: 1, noteMode: false, nrpn: false },
          hasMidiMirror: true,
        },
        {
          key: "demo-seq",
          layoutId: 1,
          startChannel: 2,
          width: 4,
          app: {
            appId: 5,
            channels: 4,
            name: "Seq (demo)",
            description: "Synthetic notes",
            color: "Orange",
            params: [],
          },
          midi: { usbEnabled: true, channel: 2, cc: null, noteMode: true, nrpn: false },
          hasMidiMirror: true,
        },
        {
          key: "demo-rnd",
          layoutId: 2,
          startChannel: 6,
          width: 1,
          app: {
            appId: 4,
            channels: 1,
            name: "RND (demo)",
            description: "Synthetic random CC",
            color: "Violet",
            params: [],
          },
          midi: { usbEnabled: true, channel: 3, cc: 16, noteMode: false, nrpn: false },
          hasMidiMirror: true,
        },
      ];

      const tracks: TrackRuntime[] = fakeTracks.map((track) => {
        const ring = new SampleRing(2048);
        audioEngine.registerTrack(track.key, track.midi.noteMode ? "note" : "cc", ring);
        return {
          key: track.key,
          track,
          ring,
          muted: false,
          solo: false,
          selected: true,
          activity: 0,
          lastEvent: null,
          unmatchedHint: null,
        };
      });

      let phase = 0;
      let step = 0;
      demoTimer = setInterval(() => {
        phase += 0.08;
        const t = performance.now();
        get().ingest({
          t,
          kind: "cc",
          channel: 1,
          cc: 1,
          value: (Math.sin(phase) + 1) / 2,
          rawValue: Math.floor(((Math.sin(phase) + 1) / 2) * 127),
        });
        get().ingest({
          t,
          kind: "cc",
          channel: 3,
          cc: 16,
          value: Math.random(),
          rawValue: Math.floor(Math.random() * 127),
        });
        if (step % 8 === 0) {
          const note = 48 + (step % 32);
          get().ingest({
            t,
            kind: "noteOn",
            channel: 2,
            note,
            velocity: 100,
            value: 100 / 127,
            rawValue: 100,
          });
          setTimeout(() => {
            get().ingest({
              t: performance.now(),
              kind: "noteOff",
              channel: 2,
              note,
              velocity: 0,
              value: 0,
            });
          }, 120);
        }
        step++;
      }, 30);

      set({
        status: "ready",
        demo: true,
        version: "demo",
        tracks,
        focusKey: tracks[0].key,
        viewMode: "all",
        error: null,
      });
    });
  },

  setViewMode: (viewMode) => set({ viewMode }),
  setFocus: (focusKey) => set({ focusKey, viewMode: focusKey ? "solo" : get().viewMode }),

  toggleMute: (key) => {
    set((s) => {
      const tracks = s.tracks.map((tr) =>
        tr.key === key ? { ...tr, muted: !tr.muted } : tr,
      );
      const tr = tracks.find((x) => x.key === key);
      if (tr) audioEngine.setTrackState(key, { muted: tr.muted, solo: tr.solo });
      return { tracks };
    });
  },

  toggleSolo: (key) => {
    set((s) => {
      const tracks = s.tracks.map((tr) =>
        tr.key === key ? { ...tr, solo: !tr.solo } : tr,
      );
      for (const tr of tracks) {
        audioEngine.setTrackState(tr.key, { muted: tr.muted, solo: tr.solo });
      }
      return { tracks, focusKey: key, viewMode: "solo" as ViewMode };
    });
  },

  toggleCompare: (key) => {
    set((s) => {
      const tracks = s.tracks.map((tr) =>
        tr.key === key ? { ...tr, selected: !tr.selected } : tr,
      );
      const selected = tracks.filter((t) => t.selected).length;
      return {
        tracks,
        viewMode: selected >= 1 ? ("compare" as ViewMode) : s.viewMode,
      };
    });
  },

  setMasterGain: (masterGain) => {
    audioEngine.setMasterGain(masterGain);
    set({ masterGain });
  },

  setWaveRate: (waveRate) => {
    audioEngine.setWaveRate(waveRate);
    set({ waveRate });
  },

  refreshParams: async () => {
    if (!snapshot || get().demo) return;
    const updated = await refreshTrackParams(snapshot);
    snapshot = { ...snapshot, tracks: updated };
    set((s) => ({
      tracks: s.tracks.map((tr) => {
        const next = updated.find((u: AppTrack) => u.key === tr.key);
        return next
          ? {
              ...tr,
              track: next,
              unmatchedHint: next.midi.usbEnabled
                ? null
                : next.hasMidiMirror
                  ? "USB MIDI out disabled on device — enable MidiOut→USB in configurator"
                  : "No MIDI mirror (CV-only app)",
            }
          : tr;
      }),
    }));
  },

  ingest: (ev) => {
    if (ev.kind === "clock") {
      set((s) => ({ clockCount: s.clockCount + 1 }));
      return;
    }

    const state = get();
    const matches = routeEvent(state.tracks, ev);
    if (matches.length === 0) {
      if (ev.kind === "cc" || ev.kind === "noteOn" || ev.kind === "nrpn") {
        set((s) => ({
          unmappedLog: [...s.unmappedLog.slice(-40), ev],
        }));
      }
      return;
    }

    const vis = visibleKeys(state);
    for (const match of matches) {
      // Always update rings for accurate profiles; audio respects mute/solo
      audioEngine.handle(match.key, ev);
      // If view filters visually we still keep data; canvas will hide
      void vis;
    }

    set((s) => ({
      tracks: s.tracks.map((tr) => {
        if (!matches.some((m) => m.key === tr.key)) {
          return { ...tr, activity: tr.activity * 0.92 };
        }
        return {
          ...tr,
          activity: 1,
          lastEvent: ev,
        };
      }),
    }));
  },
}));
