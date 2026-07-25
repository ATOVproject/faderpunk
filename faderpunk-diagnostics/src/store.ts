import { create } from "zustand";

import { audioEngine } from "./audio/engine";
import { SampleRing } from "./audio/sample-ring";
import type { AppTrack, MidiCollision, Snapshot } from "./mapping/tracks";
import {
  assignUniqueMidiChannels,
  countUsbEnabled,
  enableUsbMidiOnAll,
  ensureUsbOutputLocal,
  findMidiCollisions,
  loadSnapshot,
  refreshTrackParams,
} from "./mapping/tracks";
import { bindMidiHandlers, unbindMidiHandlers } from "./midi/device";
import { echoMidiToDevice } from "./midi/loopback";
import { sendMidiPanic } from "./midi/panic";
import { PerformanceParser, type MidiEvent } from "./midi/performance";
import { sendMidiTransport } from "./midi/transport";

export type ViewMode = "all" | "solo" | "compare";

/** One scope lane: MidiIn or one MidiOut channel (apps may have several outs). */
export interface MidiLane {
  key: string;
  role: "in" | "out";
  channel: number;
  ring: SampleRing;
}

export interface TrackRuntime {
  key: string;
  track: AppTrack;
  /** In first (if any), then each Out channel — general multi-I/O scopes. */
  lanes: MidiLane[];
  muted: boolean;
  solo: boolean;
  selected: boolean;
  activity: number;
  lastEvent: MidiEvent | null;
  unmatchedHint: string | null;
  /** Shares MIDI ch(+CC/notes) with another app — wire can't tell them apart. */
  collision: boolean;
  /** Human wire id e.g. "MIDI 13 · CC16". */
  wireLabel: string;
  /** Other app names on the same wire identity. */
  collisionPeers: string[];
  /** Collision group index for matching stripe colors (0-based). */
  collisionGroup: number;
  /** Last routed event matched multiple apps. */
  ambiguousHit: boolean;
  /** 0–1 activity on the MidiIn lane (if any). */
  inputLevel: number;
}

interface DiagState {
  status: "idle" | "connecting" | "ready" | "error";
  error: string | null;
  notice: string | null;
  version: string | null;
  demo: boolean;
  viewMode: ViewMode;
  focusKey: string | null;
  masterGain: number;
  /** CC monitor pitch control (maps to Hz via waveRateToCcPitchHz). */
  waveRate: number;
  playing: boolean;
  /** Last transport we sent (device may also emit its own). */
  transportRunning: boolean;
  tracks: TrackRuntime[];
  unmappedLog: MidiEvent[];
  clockCount: number;
  ccCount: number;
  noteCount: number;
  portSummary: string | null;
  usbOn: number;
  usbCapable: number;
  collisions: MidiCollision[];
  /** Host always echoes USB-Out → USB-In (no on-device USB loop). */
  loopbackCount: number;
  busRing: SampleRing;
  connect: () => Promise<void>;
  disconnect: () => void;
  startDemo: () => void;
  setViewMode: (m: ViewMode) => void;
  setFocus: (key: string | null) => void;
  toggleMute: (key: string) => void;
  /** Mute every track, or unmute all if every track is already muted. */
  toggleMuteAll: () => void;
  toggleSolo: (key: string) => void;
  toggleCompare: (key: string) => void;
  setMasterGain: (v: number) => void;
  setWaveRate: (v: number) => void;
  setPlaying: (on: boolean) => void;
  togglePlaying: () => void;
  panic: () => void;
  transportStart: () => void;
  transportStop: () => void;
  refreshParams: () => Promise<void>;
  enableUsbMidi: () => Promise<void>;
  uniqueMidiChannels: () => Promise<void>;
  ingest: (ev: MidiEvent) => void;
}

let snapshot: Snapshot | null = null;
const parser = new PerformanceParser();
let demoTimer: ReturnType<typeof setInterval> | null = null;
const sharedBusRing = new SampleRing(2048);

function routeEvent(tracks: TrackRuntime[], ev: MidiEvent): {
  matches: TrackRuntime[];
  ambiguous: boolean;
} {
  if (ev.kind === "clock" || ev.kind === "transport" || ev.channel === 0) {
    return { matches: [], ambiguous: false };
  }

  const onChannel = tracks.filter(
    (tr) =>
      tr.track.midi.channel === ev.channel ||
      tr.track.midi.outChannels.includes(ev.channel),
  );
  if (onChannel.length === 0) return { matches: [], ambiguous: false };

  if (ev.kind === "cc" || ev.kind === "nrpn") {
    const byCc = onChannel.filter(
      (tr) =>
        !tr.track.midi.noteMode &&
        tr.track.midi.cc !== null &&
        ev.cc !== undefined &&
        tr.track.midi.cc === ev.cc,
    );
    if (byCc.length > 0) {
      return { matches: byCc, ambiguous: byCc.length > 1 };
    }
    // CC-less continuous apps on this channel (only safe if exactly one)
    const openCc = onChannel.filter((tr) => !tr.track.midi.noteMode && tr.track.midi.cc === null);
    if (openCc.length === 1) return { matches: openCc, ambiguous: false };
    // Ambiguous or none — don't guess across multiple apps
    if (openCc.length > 1) return { matches: openCc, ambiguous: true };
    return { matches: [], ambiguous: false };
  }

  if (ev.kind === "noteOn" || ev.kind === "noteOff") {
    // Prefer note-mode apps; fall back to any app on this out channel
    // (Mode Enum mis-read or Note+CC apps still emit notes).
    const noteTracks = onChannel.filter((tr) => tr.track.midi.noteMode);
    const pool = noteTracks.length > 0 ? noteTracks : onChannel;
    return { matches: pool, ambiguous: pool.length > 1 };
  }

  return { matches: [], ambiguous: false };
}

function stopDemo() {
  if (demoTimer) {
    clearInterval(demoTimer);
    demoTimer = null;
  }
}

function detachPerf() {
  if (snapshot) unbindMidiHandlers(snapshot.device);
}

function hintFor(track: AppTrack, colliding: boolean): string | null {
  if (colliding) return null; // shown via dedicated shared-MIDI banner on the card
  if (track.midi.usbEnabled) return null;
  if (track.hasMidiMirror) {
    return "USB MIDI out off — click “Enable USB MIDI” above";
  }
  return "No MIDI mirror (CV-only app)";
}

function wireLabelFor(track: AppTrack): string {
  const { midi } = track;
  const outs =
    midi.outChannels.length > 1
      ? midi.outChannels.join("/")
      : String(midi.channel);
  const out = midi.noteMode
    ? `Out ${outs} notes`
    : midi.cc !== null
      ? `Out ${outs} CC${midi.cc}`
      : `Out ${outs}`;
  if (midi.inChannel !== null) {
    const inUsb = midi.inUsb ? "USB" : "—";
    return `In ${midi.inChannel}(${inUsb}) · ${out}`;
  }
  if (midi.noteMode) return `MIDI ${outs} · notes`;
  if (midi.cc !== null) {
    return `MIDI ${outs} · CC${midi.cc}${midi.nrpn ? " NRPN" : ""}`;
  }
  return `MIDI ${outs}`;
}

function pushVoiceToRing(
  ring: { push: (v: number, t: number) => void },
  ev: MidiEvent,
): void {
  if (ev.kind === "cc" || ev.kind === "nrpn") ring.push(ev.value ?? 0, ev.t);
  else if (ev.kind === "noteOn") ring.push(ev.value ?? 0.8, ev.t);
  else if (ev.kind === "noteOff") ring.push(0, ev.t);
}

function buildLanes(track: AppTrack, prev?: MidiLane[]): MidiLane[] {
  const reuse = (key: string) => prev?.find((l) => l.key === key)?.ring ?? new SampleRing(2048);
  const lanes: MidiLane[] = [];
  if (track.midi.inChannel !== null) {
    const key = `in:${track.midi.inChannel}`;
    lanes.push({
      key,
      role: "in",
      channel: track.midi.inChannel,
      ring: reuse(key),
    });
  }
  for (const ch of track.midi.outChannels) {
    const key = `out:${ch}`;
    lanes.push({
      key,
      role: "out",
      channel: ch,
      ring: reuse(key),
    });
  }
  if (lanes.length === 0) {
    const key = `out:${track.midi.channel}`;
    lanes.push({
      key,
      role: "out",
      channel: track.midi.channel,
      ring: reuse(key),
    });
  }
  return lanes;
}

function primaryOutRing(lanes: MidiLane[]): SampleRing {
  return lanes.find((l) => l.role === "out")?.ring ?? lanes[0]!.ring;
}

function outLaneForChannel(lanes: MidiLane[], channel: number): MidiLane | undefined {
  return lanes.find((l) => l.role === "out" && l.channel === channel);
}

function inLane(lanes: MidiLane[]): MidiLane | undefined {
  return lanes.find((l) => l.role === "in");
}

function collisionMeta(tracks: AppTrack[]): {
  collisions: MidiCollision[];
  byKey: Map<string, { peers: string[]; group: number; wire: string }>;
} {
  const collisions = findMidiCollisions(tracks);
  const byKey = new Map<string, { peers: string[]; group: number; wire: string }>();
  collisions.forEach((c, group) => {
    const names = new Map(tracks.map((t) => [t.key, t.app.name]));
    for (const key of c.trackKeys) {
      const peers = c.trackKeys
        .filter((k) => k !== key)
        .map((k) => names.get(k) ?? k);
      const track = tracks.find((t) => t.key === key);
      byKey.set(key, {
        peers,
        group,
        wire: track ? wireLabelFor(track) : c.key,
      });
    }
  });
  return { collisions, byKey };
}

function applyTrackUpdates(
  current: TrackRuntime[],
  updated: AppTrack[],
): { tracks: TrackRuntime[]; collisions: MidiCollision[] } {
  const { collisions, byKey } = collisionMeta(updated);
  return {
    collisions,
    tracks: current.map((tr) => {
      const next = updated.find((u) => u.key === tr.key);
      if (!next) return tr;
      const meta = byKey.get(tr.key);
      const collision = Boolean(meta);
      const lanes = buildLanes(next, tr.lanes);
      audioEngine.registerTrack(tr.key, next.midi.noteMode ? "note" : "cc", primaryOutRing(lanes));
      return {
        ...tr,
        track: next,
        lanes,
        collision,
        wireLabel: wireLabelFor(next),
        collisionPeers: meta?.peers ?? [],
        collisionGroup: meta?.group ?? -1,
        unmatchedHint: hintFor(next, collision),
      };
    }),
  };
}

function buildTrackRuntimes(tracks: AppTrack[]): {
  runtimes: TrackRuntime[];
  collisions: MidiCollision[];
} {
  const { collisions, byKey } = collisionMeta(tracks);
  const runtimes = tracks.map((track) => {
    const lanes = buildLanes(track);
    const kind = track.midi.noteMode ? "note" : "cc";
    audioEngine.registerTrack(track.key, kind, primaryOutRing(lanes));
    const meta = byKey.get(track.key);
    const collision = Boolean(meta);
    return {
      key: track.key,
      track,
      lanes,
      muted: false,
      solo: false,
      selected: false,
      activity: 0,
      lastEvent: null,
      unmatchedHint: hintFor(track, collision),
      collision,
      wireLabel: wireLabelFor(track),
      collisionPeers: meta?.peers ?? [],
      collisionGroup: meta?.group ?? -1,
      ambiguousHit: false,
      inputLevel: 0,
    };
  });
  return { runtimes, collisions };
}

function collisionNotice(collisions: MidiCollision[]): string | null {
  if (collisions.length === 0) return null;
  return `${collisions.length} MIDI collision(s): ${collisions.map((c) => c.label).join("; ")}. Unique MIDI channels to split.`;
}

export const useDiag = create<DiagState>((set, get) => ({
  status: "idle",
  error: null,
  notice: null,
  version: null,
  demo: false,
  viewMode: "all",
  focusKey: null,
  masterGain: 0.35,
  waveRate: 8,
  playing: true,
  transportRunning: false,
  tracks: [],
  unmappedLog: [],
  clockCount: 0,
  ccCount: 0,
  noteCount: 0,
  portSummary: null,
  usbOn: 0,
  usbCapable: 0,
  collisions: [],
  loopbackCount: 0,
  busRing: sharedBusRing,

  connect: async () => {
    stopDemo();
    detachPerf();
    audioEngine.unregisterAll();
    sharedBusRing.clear();
    set({ status: "connecting", error: null, notice: null, demo: false });
    try {
      const snap = await loadSnapshot();
      snapshot = snap;

      const usbFix = await ensureUsbOutputLocal(snap);
      await audioEngine.ensure();
      audioEngine.setMasterGain(get().masterGain);
      audioEngine.setWaveRate(get().waveRate);
      audioEngine.setPlaying(true);

      const { runtimes: tracks, collisions } = buildTrackRuntimes(snap.tracks);

      bindMidiHandlers(snap.device, (data, t) => {
        // Always host-echo performance MIDI → device USB-In (cable 0).
        const outs = [
          ...snap.device.performanceOutputs,
          snap.device.config.output,
        ];
        echoMidiToDevice(outs, data);
        if (data.length > 0 && data[0] < 0xf0) {
          set((s) => ({ loopbackCount: s.loopbackCount + 1 }));
        }
        const events = parser.parse(data, t);
        for (const ev of events) get().ingest(ev);
      });

      const usb = countUsbEnabled(snap.tracks);
      const noticeParts: string[] = [];
      if (usbFix) noticeParts.push(usbFix);
      const onlyConfigOut = snap.device.performanceOutputs.every(
        (o) => o.id === snap.device.config.output.id,
      );
      if (onlyConfigOut) {
        noticeParts.push(
          "Only the config MIDI port is visible — host echo may not reach MidiIn (cable 1 ignores notes). Check OS MIDI ports.",
        );
      } else {
        noticeParts.push("Host USB echo on — MidiIn apps hear other apps’ USB Out.");
      }
      if (usb.capable > 0 && usb.on === 0) {
        noticeParts.push(
          "No app has MidiOut→USB enabled — scopes stay flat until you enable it.",
        );
      }
      if (collisions.length > 0) {
        noticeParts.push(
          `${collisions.length} MIDI collision(s): same ch/CC can’t be told apart. Use Unique MIDI channels.`,
        );
      }

      set({
        status: "ready",
        version: snap.version,
        tracks,
        collisions,
        focusKey: tracks[0]?.key ?? null,
        unmappedLog: [],
        clockCount: 0,
        ccCount: 0,
        noteCount: 0,
        playing: true,
        portSummary: snap.device.portSummary,
        usbOn: usb.on,
        usbCapable: usb.capable,
        loopbackCount: 0,
        notice: noticeParts.length ? noticeParts.join(" ") : null,
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
    sharedBusRing.clear();
    set({
      status: "idle",
      version: null,
      tracks: [],
      demo: false,
      unmappedLog: [],
      notice: null,
      portSummary: null,
      usbOn: 0,
      usbCapable: 0,
      collisions: [],
      loopbackCount: 0,
    });
  },

  startDemo: () => {
    stopDemo();
    detachPerf();
    audioEngine.unregisterAll();
    sharedBusRing.clear();
    void audioEngine.ensure().then(() => {
      audioEngine.setMasterGain(get().masterGain);
      audioEngine.setWaveRate(get().waveRate);
      audioEngine.setPlaying(true);
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
          midi: {
            usbEnabled: true,
            channel: 1,
            outChannels: [1],
            inChannel: null,
            inUsb: null,
            cc: 1,
            noteMode: false,
            nrpn: false,
          },
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
          midi: {
            usbEnabled: true,
            channel: 2,
            outChannels: [2],
            inChannel: null,
            inUsb: null,
            cc: null,
            noteMode: true,
            nrpn: false,
          },
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
          midi: {
            usbEnabled: true,
            channel: 3,
            outChannels: [3],
            inChannel: null,
            inUsb: null,
            cc: 16,
            noteMode: false,
            nrpn: false,
          },
          hasMidiMirror: true,
        },
      ];

      const tracks: TrackRuntime[] = fakeTracks.map((track) => {
        const lanes = buildLanes(track);
        audioEngine.registerTrack(track.key, track.midi.noteMode ? "note" : "cc", primaryOutRing(lanes));
        return {
          key: track.key,
          track,
          lanes,
          muted: false,
          solo: false,
          selected: true,
          activity: 0,
          lastEvent: null,
          unmatchedHint: null,
          collision: false,
          wireLabel: wireLabelFor(track),
          collisionPeers: [],
          collisionGroup: -1,
          ambiguousHit: false,
          inputLevel: 0,
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
        notice: null,
        playing: true,
        usbOn: 3,
        usbCapable: 3,
        collisions: [],
        portSummary: "demo",
        clockCount: 0,
        ccCount: 0,
        noteCount: 0,
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

  toggleMuteAll: () => {
    const tracks = get().tracks;
    if (tracks.length === 0) return;
    const allMuted = tracks.every((tr) => tr.muted);
    const muted = !allMuted;
    set({
      tracks: tracks.map((tr) => ({ ...tr, muted })),
      notice: muted ? "All tracks muted" : "All tracks unmuted",
    });
    for (const tr of get().tracks) {
      audioEngine.setTrackState(tr.key, { muted: tr.muted, solo: tr.solo });
    }
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

  setPlaying: (on) => {
    audioEngine.setPlaying(on);
    set({ playing: on });
  },

  togglePlaying: () => {
    const playing = audioEngine.togglePlaying();
    set({ playing });
  },

  panic: () => {
    audioEngine.panic();
    // Re-open monitor gate; silence is via per-track M (all muted)
    audioEngine.setPlaying(true);
    set((s) => {
      sharedBusRing.clear();
      if (snapshot && !get().demo) {
        sendMidiTransport(snapshot.device.performanceOutputs, "stop");
        sendMidiPanic(snapshot.device.performanceOutputs);
      }
      const tracks = s.tracks.map((tr) => {
        for (const lane of tr.lanes) lane.ring.clear();
        return { ...tr, activity: 0, lastEvent: null, inputLevel: 0, muted: true };
      });
      for (const tr of tracks) {
        audioEngine.setTrackState(tr.key, { muted: true, solo: tr.solo });
      }
      return {
        playing: true,
        transportRunning: false,
        tracks,
        notice: "Panic — all muted. Space = Unmute all. MIDI Stop + All Notes Off sent.",
      };
    });
  },

  transportStart: () => {
    if (snapshot && !get().demo) {
      sendMidiTransport(snapshot.device.performanceOutputs, "start");
    }
    set({
      transportRunning: true,
      notice: get().demo
        ? "Demo has no device transport"
        : "MIDI Start sent (device follows if Clock Src = MIDI USB)",
    });
  },

  transportStop: () => {
    if (snapshot && !get().demo) {
      sendMidiTransport(snapshot.device.performanceOutputs, "stop");
    }
    set({
      transportRunning: false,
      notice: get().demo ? "Demo has no device transport" : "MIDI Stop sent",
    });
  },

  refreshParams: async () => {
    if (!snapshot || get().demo) return;
    const updated = await refreshTrackParams(snapshot);
    snapshot = { ...snapshot, tracks: updated };
    const usb = countUsbEnabled(updated);
    set((s) => {
      const { tracks, collisions } = applyTrackUpdates(s.tracks, updated);
      return {
        tracks,
        collisions,
        usbOn: usb.on,
        usbCapable: usb.capable,
        notice:
          collisionNotice(collisions) ??
          (usb.capable > 0 && usb.on === 0
            ? "No app has MidiOut→USB enabled — scopes stay flat until you enable it."
            : null),
      };
    });
  },

  enableUsbMidi: async () => {
    if (!snapshot || get().demo) return;
    set({ notice: "Enabling MidiOut→USB on apps…" });
    try {
      const usbFix = await ensureUsbOutputLocal(snapshot);
      const changed = await enableUsbMidiOnAll(snapshot);
      const updated = await refreshTrackParams(snapshot);
      snapshot = { ...snapshot, tracks: updated };
      const usb = countUsbEnabled(updated);
      set((s) => {
        const { tracks, collisions } = applyTrackUpdates(s.tracks, updated);
        return {
          tracks,
          collisions,
          usbOn: usb.on,
          usbCapable: usb.capable,
          notice: [
            usbFix,
            changed > 0
              ? `Enabled USB MIDI on ${changed} app(s). Waves should appear if those apps are running.`
              : "All capable apps already had USB MIDI on.",
            collisionNotice(collisions),
          ]
            .filter(Boolean)
            .join(" "),
        };
      });
    } catch (err) {
      set({
        notice: null,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  uniqueMidiChannels: async () => {
    if (!snapshot || get().demo) return;
    set({
      notice:
        "Assigning unique MIDI channels on colliding apps only… (close the Configurator first — shared SysEx cable)",
    });
    try {
      const changed = await assignUniqueMidiChannels(snapshot);
      const updated = await refreshTrackParams(snapshot);
      snapshot = { ...snapshot, tracks: updated };
      set((s) => {
        const { tracks, collisions } = applyTrackUpdates(s.tracks, updated);
        return {
          tracks,
          collisions,
          notice: [
            changed > 0
              ? `Split ${changed} colliding app(s) onto unique MIDI channels.`
              : "No colliding apps to split (or no free channels).",
            collisionNotice(collisions),
            "If the Configurator was open, reconnect it — it shares the config MIDI cable.",
          ]
            .filter(Boolean)
            .join(" "),
        };
      });
    } catch (err) {
      set({
        notice: null,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  ingest: (ev) => {
    if (ev.kind === "transport") {
      // 0xFA start, 0xFB continue, 0xFC stop
      if (ev.rawValue === 0xfa || ev.rawValue === 0xfb) {
        set({ transportRunning: true });
      } else if (ev.rawValue === 0xfc) {
        set({ transportRunning: false });
      }
      return;
    }

    if (ev.kind === "clock") {
      set((s) => ({ clockCount: s.clockCount + 1 }));
      return;
    }

    if (ev.kind === "cc" || ev.kind === "nrpn") {
      sharedBusRing.push(ev.value ?? 0, ev.t);
      set((s) => ({ ccCount: s.ccCount + 1 }));
    } else if (ev.kind === "noteOn") {
      sharedBusRing.push(ev.value ?? 0.8, ev.t);
      set((s) => ({ noteCount: s.noteCount + 1 }));
    } else if (ev.kind === "noteOff") {
      sharedBusRing.push(0, ev.t);
    }

    const state = get();
    const { matches, ambiguous } = routeEvent(state.tracks, ev);

    const inAmp =
      ev.kind === "noteOff"
        ? 0
        : Math.max(0, Math.min(1, ev.value ?? (ev.kind === "noteOn" ? 0.85 : 0)));
    const isVoice =
      ev.kind === "cc" || ev.kind === "nrpn" || ev.kind === "noteOn" || ev.kind === "noteOff";

    // In lanes: bus traffic on MidiIn CH (host always echoes → device MidiIn)
    if (isVoice) {
      for (const tr of state.tracks) {
        const inn = inLane(tr.lanes);
        if (inn && inn.channel === ev.channel) pushVoiceToRing(inn.ring, ev);
      }
    }

    if (matches.length === 0) {
      if (isVoice) {
        set((s) => ({
          unmappedLog:
            ev.kind === "cc" || ev.kind === "noteOn" || ev.kind === "nrpn"
              ? [...s.unmappedLog.slice(-40), ev]
              : s.unmappedLog,
          tracks: s.tracks.map((tr) => {
            const inHit =
              tr.track.midi.inChannel !== null && tr.track.midi.inChannel === ev.channel;
            return {
              ...tr,
              activity: inHit ? Math.max(tr.activity, 0.5) : tr.activity * 0.92,
              ambiguousHit: false,
              inputLevel: inHit
                ? Math.max(tr.inputLevel * 0.5, inAmp)
                : tr.inputLevel * 0.88,
            };
          }),
        }));
      }
      return;
    }

    // Record on the matching Out lane; audio when attribution is unique
    for (const match of matches) {
      const lane = outLaneForChannel(match.lanes, ev.channel);
      if (lane) pushVoiceToRing(lane.ring, ev);
      if (!ambiguous) audioEngine.handle(match.key, ev, false);
    }

    set((s) => ({
      tracks: s.tracks.map((tr) => {
        const inHit =
          isVoice &&
          tr.track.midi.inChannel !== null &&
          tr.track.midi.inChannel === ev.channel;
        const inputLevel = inHit
          ? Math.max(tr.inputLevel * 0.5, inAmp)
          : tr.inputLevel * 0.88;

        if (!matches.some((m) => m.key === tr.key)) {
          return {
            ...tr,
            activity: inHit ? Math.max(tr.activity, 0.45) : tr.activity * 0.92,
            ambiguousHit: false,
            inputLevel,
          };
        }
        return {
          ...tr,
          activity: 1,
          lastEvent: ev,
          ambiguousHit: ambiguous,
          inputLevel,
        };
      }),
    }));
  },
}));
