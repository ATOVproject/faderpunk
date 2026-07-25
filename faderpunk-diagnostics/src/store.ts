import { create } from "zustand";

import { audioEngine } from "./audio/engine";
import {
  DEFAULT_MONITOR_NOTE,
  clampKeyPc,
  monitorMidiNote,
  type MonitorNote,
} from "./audio/music";
import { SampleRing } from "./audio/sample-ring";
import type { AppTrack, MidiCollision, Snapshot } from "./mapping/tracks";
import {
  applyAppStateToTrack,
  assignUniqueMidiChannels,
  countUsbEnabled,
  enableUsbMidiOnAll,
  ensureMidiUsbClockSource,
  ensureUsbOutputLocal,
  findMidiCollisions,
  loadSnapshot,
  readClockConfig,
  refreshAppParamsOnly,
  reloadTracks,
  writeDeviceBpm,
  clampBpm,
} from "./mapping/tracks";
import {
  bindMidiHandlers,
  releaseDevice,
  unbindMidiHandlers,
  type ConfigPushHandler,
} from "./midi/device";
import type { ConfigMsgOut } from "@atov/fp-config";
import { hostClock } from "./midi/host-clock";
import { echoMidiToDevice } from "./midi/loopback";
import { sendMidiPanic, sendMidiPanicChannels } from "./midi/panic";
import { PerformanceParser, type MidiEvent } from "./midi/performance";
import { sendMidiTransport } from "./midi/transport";

export type ViewMode = "all" | "solo" | "compare";

/** One scope lane: MidiIn or one MidiOut channel (apps may have several outs). */
export interface MidiLane {
  key: string;
  role: "in" | "out";
  channel: number;
  ring: SampleRing;
  /** CC carrier note for this out — independent per MIDI out. */
  monitorNote?: MonitorNote;
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
  /** Global musical key (pitch class 0=C … 11=B) for CC monitor notes. */
  keyPc: number;
  /** Device clock source tag (Internal / MidiUsb / …). */
  clockSrc: string | null;
  /** Host/device tempo used for MIDI clock ticks. */
  clockBpm: number;
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
  /** User dismissed the shared-MIDI conflict banner (per session / until reconnect). */
  collisionsBannerDismissed: boolean;
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
  setKeyPc: (pc: number) => void;
  /** Host MIDI clock tempo (also written to device internal_bpm). */
  setClockBpm: (bpm: number) => void;
  setLaneMonitorNote: (trackKey: string, laneKey: string, note: MonitorNote) => void;
  setPlaying: (on: boolean) => void;
  togglePlaying: () => void;
  panic: () => void;
  /** All Notes/Sound Off on this track’s MIDI out channels + kill its monitor voices. */
  panicTrack: (key: string) => void;
  transportStart: () => Promise<void>;
  transportStop: () => void;
  refreshParams: () => Promise<void>;
  enableUsbMidi: () => Promise<void>;
  uniqueMidiChannels: () => Promise<void>;
  dismissCollisionsBanner: () => void;
  ingest: (ev: MidiEvent) => void;
}

let snapshot: Snapshot | null = null;
const parser = new PerformanceParser();
let demoTimer: ReturnType<typeof setInterval> | null = null;
let bpmWriteTimer: ReturnType<typeof setTimeout> | null = null;
let paramsPollTimer: ReturnType<typeof setInterval> | null = null;
let paramsPollBusy = false;
/** Serialize config SysEx so soft-poll can't steal Unique MIDI / GetAppParams replies. */
let configLock: Promise<void> = Promise.resolve();
/** Dense CC (e.g. Heat Pump / Super LFO) can exceed 1k events/s — keep ≥8s headroom. */
const SCOPE_RING_CAPACITY = 16384;
const sharedBusRing = new SampleRing(SCOPE_RING_CAPACITY);
const PARAMS_POLL_MS = 2800;

function withConfigLock<T>(fn: () => Promise<T>): Promise<T> {
  const run = configLock.then(fn, fn);
  configLock = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

function stopParamsPoll() {
  if (paramsPollTimer) {
    clearInterval(paramsPollTimer);
    paramsPollTimer = null;
  }
  paramsPollBusy = false;
}

function paramRowsEqual(a: AppTrack, b: AppTrack): boolean {
  if (a.layoutId !== b.layoutId || a.key !== b.key) return false;
  if (a.midi.channel !== b.midi.channel) return false;
  if (a.midi.noteMode !== b.midi.noteMode || a.midi.playCc !== b.midi.playCc) {
    return false;
  }
  if (a.app.color !== b.app.color) return false;
  if (a.paramRows.length !== b.paramRows.length) return false;
  return a.paramRows.every(
    (row, i) =>
      row.name === b.paramRows[i]?.name && row.text === b.paramRows[i]?.text,
  );
}

function tracksVisiblyEqual(prev: AppTrack[], next: AppTrack[]): boolean {
  if (prev.length !== next.length) return false;
  return prev.every((t, i) => {
    const n = next[i];
    return n !== undefined && paramRowsEqual(t, n);
  });
}

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
        tr.track.midi.playCc &&
        tr.track.midi.cc !== null &&
        ev.cc !== undefined &&
        tr.track.midi.cc === ev.cc,
    );
    if (byCc.length > 0) {
      return { matches: byCc, ambiguous: byCc.length > 1 };
    }
    // CC-less continuous apps on this channel (only safe if exactly one)
    const openCc = onChannel.filter((tr) => tr.track.midi.playCc && tr.track.midi.cc === null);
    if (openCc.length === 1) return { matches: openCc, ambiguous: false };
    // Ambiguous or none — don't guess across multiple apps
    if (openCc.length > 1) return { matches: openCc, ambiguous: true };
    return { matches: [], ambiguous: false };
  }

  if (ev.kind === "noteOn" || ev.kind === "noteOff") {
    // Prefer note-mode / hybrid apps; fall back to any app on this out channel
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

async function releaseSnapshot(): Promise<void> {
  stopDemo();
  stopParamsPoll();
  if (bpmWriteTimer) {
    clearTimeout(bpmWriteTimer);
    bpmWriteTimer = null;
  }
  hostClock.halt();
  const snap = snapshot;
  snapshot = null;
  if (snap) {
    try {
      await releaseDevice(snap.device);
    } catch {
      unbindMidiHandlers(snap.device);
    }
  }
  audioEngine.unregisterAll();
  sharedBusRing.clear();
}

function hintFor(track: AppTrack, colliding: boolean): string | null {
  if (colliding) return null; // share-banner
  if (!track.hasMidiMirror) return null; // cv-banner
  if (track.midi.usbEnabled) return null;
  return "This app isn’t sending MIDI over USB — click Enable USB MIDI above";
}

function wireLabelFor(track: AppTrack): string {
  const { midi } = track;
  const outs =
    midi.outChannels.length > 1
      ? midi.outChannels.join("/")
      : String(midi.channel);
  const noteHint =
    midi.setupNotes.length > 0
      ? ` n${midi.setupNotes.join("/")}`
      : " notes";
  const ccHint =
    midi.cc !== null ? ` CC${midi.cc}${midi.nrpn ? " NRPN" : ""}` : " CC";
  const outPorts: string[] = [];
  if (midi.usbEnabled) outPorts.push("USB");
  if (midi.out1) outPorts.push("1");
  if (midi.out2) outPorts.push("2");
  const portHint = outPorts.length ? ` →${outPorts.join("+")}` : "";
  let out: string;
  if (midi.noteMode && midi.playCc) {
    out = `Out ${outs}${noteHint}+${ccHint.trim()}${portHint}`;
  } else if (midi.noteMode) {
    out = `Out ${outs}${noteHint}${portHint}`;
  } else {
    out = `Out ${outs}${ccHint}${portHint}`;
  }
  if (midi.inChannel !== null) {
    const inPorts: string[] = [];
    if (midi.inUsb) inPorts.push("USB");
    if (midi.inDin) inPorts.push("DIN");
    const inHint = inPorts.length ? inPorts.join("+") : "—";
    return `In ${midi.inChannel}(${inHint}) · ${out}`;
  }
  if (midi.noteMode && midi.playCc) {
    return `MIDI ${outs} · notes+CC${portHint}`;
  }
  if (midi.noteMode) return `MIDI ${outs} · notes${portHint}`;
  if (midi.cc !== null) {
    return `MIDI ${outs} · CC${midi.cc}${midi.nrpn ? " NRPN" : ""}${portHint}`;
  }
  return `MIDI ${outs}${portHint}`;
}

function audioKindFor(midi: AppTrack["midi"]): "note" | "cc" | "hybrid" {
  if (midi.noteMode && midi.playCc) return "hybrid";
  if (midi.noteMode) return "note";
  return "cc";
}

function defaultMonitorNote(index: number): MonitorNote {
  // Stagger degrees so multiple outs / apps don't all drone the tonic
  return { degree: index % 7, octave: DEFAULT_MONITOR_NOTE.octave };
}

function syncTrackCcPitch(tr: TrackRuntime, keyPc: number) {
  if (!tr.track.midi.playCc && tr.track.midi.noteMode) return;
  for (const lane of tr.lanes) {
    if (lane.role !== "out" || !lane.monitorNote) continue;
    audioEngine.setLaneCcMidi(
      tr.key,
      lane.key,
      monitorMidiNote(keyPc, lane.monitorNote),
    );
  }
}

function registerAudio(
  tr: { key: string; track: AppTrack; lanes: MidiLane[] },
  keyPc: number,
) {
  const kind = audioKindFor(tr.track.midi);
  const outs = tr.lanes
    .filter((l) => l.role === "out")
    .map((l, i) => ({
      key: l.key,
      ring: l.ring,
      ccMidi: monitorMidiNote(
        keyPc,
        l.monitorNote ?? defaultMonitorNote(i),
      ),
    }));
  audioEngine.registerTrack(tr.key, kind, outs);
}

function pushVoiceToRing(
  ring: { push: (v: number, t: number) => void },
  ev: MidiEvent,
): void {
  if (ev.kind === "cc" || ev.kind === "nrpn") ring.push(ev.value ?? 0, ev.t);
  else if (ev.kind === "noteOn") ring.push(ev.value ?? 0.8, ev.t);
  else if (ev.kind === "noteOff") ring.push(0, ev.t);
}

function scopeRing(prior?: SampleRing): SampleRing {
  if (prior && prior.capacity >= SCOPE_RING_CAPACITY) return prior;
  return new SampleRing(SCOPE_RING_CAPACITY);
}

function buildLanes(
  track: AppTrack,
  prev?: MidiLane[],
  trackIndex = 0,
): MidiLane[] {
  const reuse = (key: string) => prev?.find((l) => l.key === key);
  const lanes: MidiLane[] = [];
  if (track.midi.inChannel !== null) {
    const key = `in:${track.midi.inChannel}`;
    const prior = reuse(key);
    lanes.push({
      key,
      role: "in",
      channel: track.midi.inChannel,
      ring: scopeRing(prior?.ring),
    });
  }
  let outIdx = 0;
  for (const ch of track.midi.outChannels) {
    const key = `out:${ch}`;
    const prior = reuse(key);
    lanes.push({
      key,
      role: "out",
      channel: ch,
      ring: scopeRing(prior?.ring),
      monitorNote:
        prior?.monitorNote ?? defaultMonitorNote(trackIndex * 3 + outIdx),
    });
    outIdx++;
  }
  if (lanes.filter((l) => l.role === "out").length === 0) {
    const key = `out:${track.midi.channel}`;
    const prior = reuse(key);
    lanes.push({
      key,
      role: "out",
      channel: track.midi.channel,
      ring: scopeRing(prior?.ring),
      monitorNote: prior?.monitorNote ?? defaultMonitorNote(trackIndex),
    });
  }
  return lanes;
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

function buildTrackRuntimes(
  tracks: AppTrack[],
  keyPc: number,
  prev?: TrackRuntime[],
): {
  runtimes: TrackRuntime[];
  collisions: MidiCollision[];
} {
  const { collisions, byKey } = collisionMeta(tracks);
  const runtimes = tracks.map((track, i) => {
    const prior = prev?.find((p) => p.key === track.key);
    const lanes = buildLanes(track, prior?.lanes, i);
    const meta = byKey.get(track.key);
    const collision = Boolean(meta);
    const runtime: TrackRuntime = {
      key: track.key,
      track,
      lanes,
      muted: prior?.muted ?? false,
      solo: prior?.solo ?? false,
      selected: prior?.selected ?? false,
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
    registerAudio(runtime, keyPc);
    return runtime;
  });
  return { runtimes, collisions };
}

function collisionNotice(collisions: MidiCollision[]): string | null {
  if (collisions.length === 0) return null;
  return `${collisions.length} MIDI collision(s): ${collisions.map((c) => c.label).join("; ")}. Unique MIDI channels to split.`;
}

function commitTracks(
  updated: AppTrack[],
  get: () => DiagState,
  set: (
    partial:
      | Partial<DiagState>
      | ((s: DiagState) => Partial<DiagState>),
  ) => void,
): void {
  if (!snapshot) return;
  snapshot = { ...snapshot, tracks: updated };
  const usb = countUsbEnabled(updated);
  const { runtimes, collisions } = buildTrackRuntimes(
    updated,
    get().keyPc,
    get().tracks,
  );
  set({
    tracks: runtimes,
    collisions,
    usbOn: usb.on,
    usbCapable: usb.capable,
  });
}

function handleConfigPush(
  msg: ConfigMsgOut,
  get: () => DiagState,
  set: (
    partial:
      | Partial<DiagState>
      | ((s: DiagState) => Partial<DiagState>),
  ) => void,
): void {
  if (!snapshot || get().demo || get().status !== "ready") return;
  if (msg.tag === "AppState") {
    const layoutId = Number(
      typeof msg.value[0] === "bigint" ? Number(msg.value[0]) : msg.value[0],
    );
    const values = msg.value[1];
    const updated = snapshot.tracks.map((t) =>
      t.layoutId === layoutId
        ? applyAppStateToTrack(t, values, snapshot!.apps)
        : t,
    );
    if (tracksVisiblyEqual(snapshot.tracks, updated)) return;
    commitTracks(updated, get, set);
    return;
  }
  if (msg.tag === "Layout") {
    // Slot map changed — full refresh (async).
    void get().refreshParams();
  }
}

async function softPollParams(
  get: () => DiagState,
  set: (
    partial:
      | Partial<DiagState>
      | ((s: DiagState) => Partial<DiagState>),
  ) => void,
): Promise<void> {
  if (!snapshot || get().demo || get().status !== "ready" || paramsPollBusy) {
    return;
  }
  if (snapshot.device.config.rx.waiter) return;
  paramsPollBusy = true;
  try {
    await withConfigLock(async () => {
      if (!snapshot || get().status !== "ready") return;
      const updated = await refreshAppParamsOnly(snapshot);
      if (!snapshot) return;
      if (tracksVisiblyEqual(snapshot.tracks, updated)) return;
      commitTracks(updated, get, set);
    });
  } catch (err) {
    console.warn("param soft-poll:", err);
  } finally {
    paramsPollBusy = false;
  }
}

function startParamsPoll(
  get: () => DiagState,
  set: (
    partial:
      | Partial<DiagState>
      | ((s: DiagState) => Partial<DiagState>),
  ) => void,
): void {
  stopParamsPoll();
  paramsPollTimer = setInterval(() => {
    void softPollParams(get, set);
  }, PARAMS_POLL_MS);
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
  keyPc: 0,
  clockSrc: null,
  clockBpm: 120,
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
  collisionsBannerDismissed: false,
  loopbackCount: 0,
  busRing: sharedBusRing,

  connect: async () => {
    await releaseSnapshot();
    set({ status: "connecting", error: null, notice: null, demo: false });
    try {
      const snap = await loadSnapshot();
      snapshot = snap;

      const usbFix = await ensureUsbOutputLocal(snap);
      const clock = await readClockConfig(snap);
      await audioEngine.ensure();
      audioEngine.setMasterGain(get().masterGain);
      audioEngine.setPlaying(true);

      hostClock.setOutputs(snap.device.performanceOutputs);
      if (clock) hostClock.setBpm(clock.bpm);

      const { runtimes: tracks, collisions } = buildTrackRuntimes(
        snap.tracks,
        get().keyPc,
      );

      bindMidiHandlers(
        snap.device,
        (data, t) => {
          // Always host-echo performance MIDI → device USB-In (cable 0),
          // unless the monitor is fully muted (panic / mute-all).
          const allMuted =
            get().tracks.length > 0 && get().tracks.every((tr) => tr.muted);
          if (!allMuted) {
            const outs = [
              ...snap.device.performanceOutputs,
              snap.device.config.output,
            ];
            echoMidiToDevice(outs, data);
            if (data.length > 0 && data[0] < 0xf0) {
              set((s) => ({ loopbackCount: s.loopbackCount + 1 }));
            }
          }
          const events = parser.parse(data, t);
          for (const ev of events) get().ingest(ev);
        },
        ((msg) => handleConfigPush(msg, get, set)) satisfies ConfigPushHandler,
      );

      startParamsPoll(get, set);

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
      if (snap.tracks.length === 0) {
        noticeParts.push(
          "Layout is empty — no app slots to scope. Push a setup from the Editor, then Reconnect.",
        );
      }
      if (clock && clock.src !== "MidiUsb") {
        noticeParts.push(
          `Clock Src is ${clock.src} — Start will switch to MIDI USB and send host clock @ ${clock.bpm} BPM.`,
        );
      } else if (clock) {
        noticeParts.push(`Clock Src MIDI USB · host tempo ${clock.bpm} BPM.`);
      }
      if (!snap.device.hasDedicatedPerfOut) {
        noticeParts.push(
          "Only config MIDI port visible — host clock/Start may not reach the device. Check OS MIDI ports (need cable 0).",
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
        collisionsBannerDismissed: false,
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
        clockSrc: clock?.src ?? null,
        clockBpm: clampBpm(clock?.bpm ?? 120),
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
    void releaseSnapshot().then(() => {
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
        collisionsBannerDismissed: false,
        loopbackCount: 0,
        transportRunning: false,
        clockSrc: null,
      });
    });
  },

  startDemo: () => {
    void releaseSnapshot().then(() => {
    void audioEngine.ensure().then(() => {
      audioEngine.setMasterGain(get().masterGain);
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
            icon: "Sine",
            params: [],
          },
          midi: {
            usbEnabled: true,
            out1: true,
            out2: false,
            channel: 1,
            outChannels: [1],
            outChannelNames: ["MIDI Out"],
            inChannel: null,
            inUsb: null,
            inDin: null,
            cc: 1,
            noteMode: false,
            playCc: true,
            setupNotes: [],
            nrpn: false,
          },
          paramRows: [
            { name: "MidiOut", kind: "MidiOut", text: "USB+Out1" },
            { name: "MIDI Channel", kind: "MidiChannel", text: "1" },
            { name: "CC number", kind: "MidiCc", text: "1" },
          ],
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
            icon: "Sequence",
            params: [],
          },
          midi: {
            usbEnabled: true,
            out1: true,
            out2: true,
            channel: 2,
            outChannels: [2],
            outChannelNames: ["MIDI Out"],
            inChannel: null,
            inUsb: null,
            inDin: null,
            cc: null,
            noteMode: true,
            playCc: false,
            setupNotes: [48],
            nrpn: false,
          },
          paramRows: [
            { name: "MidiOut", kind: "MidiOut", text: "USB+Out1+Out2" },
            { name: "MIDI Channel", kind: "MidiChannel", text: "2" },
            { name: "Base note", kind: "MidiNote", text: "48" },
          ],
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
            icon: "Random",
            params: [],
          },
          midi: {
            usbEnabled: true,
            out1: false,
            out2: false,
            channel: 3,
            outChannels: [3],
            outChannelNames: ["MIDI Out"],
            inChannel: null,
            inUsb: null,
            inDin: null,
            cc: 16,
            noteMode: false,
            playCc: true,
            setupNotes: [],
            nrpn: false,
          },
          paramRows: [
            { name: "MidiOut", kind: "MidiOut", text: "USB" },
            { name: "MIDI Channel", kind: "MidiChannel", text: "3" },
            { name: "MIDI CC", kind: "MidiCc", text: "16" },
          ],
          hasMidiMirror: true,
        },
      ];

      const { runtimes: tracks } = buildTrackRuntimes(fakeTracks, get().keyPc);
      // Demo: pre-select all for compare-ish view
      const selected = tracks.map((tr) => ({ ...tr, selected: true }));

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
        tracks: selected,
        focusKey: selected[0]?.key ?? null,
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
      if (tr && !tr.muted) audioEngine.setPlaying(true);
      return {
        tracks,
        playing: tr && !tr.muted ? true : s.playing,
        notice: tr?.muted ? `${tr.track.app.name} muted` : null,
      };
    });
  },

  toggleMuteAll: () => {
    const tracks = get().tracks;
    if (tracks.length === 0) return;
    const allMuted = tracks.every((tr) => tr.muted);
    const muted = !allMuted;
    const next = tracks.map((tr) => ({ ...tr, muted }));
    if (muted) {
      audioEngine.muteAll();
    } else {
      audioEngine.setPlaying(true);
      for (const tr of next) {
        audioEngine.setTrackState(tr.key, { muted: false, solo: tr.solo });
      }
    }
    set({
      tracks: next,
      playing: muted ? false : true,
      notice: muted
        ? "All muted — host MIDI echo paused. Space = unmute."
        : "All unmuted — host MIDI echo on.",
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

  setKeyPc: (pc) => {
    const keyPc = clampKeyPc(pc);
    set({ keyPc });
    for (const tr of get().tracks) syncTrackCcPitch(tr, keyPc);
  },

  setClockBpm: (raw) => {
    const clockBpm = clampBpm(raw);
    hostClock.setBpm(clockBpm);
    set({ clockBpm });
    if (get().demo || !snapshot) return;
    if (bpmWriteTimer) clearTimeout(bpmWriteTimer);
    bpmWriteTimer = setTimeout(() => {
      bpmWriteTimer = null;
      if (!snapshot || get().demo) return;
      void writeDeviceBpm(snapshot, clockBpm).catch((err) => {
        console.warn("Failed to write device BPM:", err);
      });
    }, 350);
  },

  setLaneMonitorNote: (trackKey, laneKey, note) => {
    const keyPc = get().keyPc;
    set((s) => {
      const tracks = s.tracks.map((tr) => {
        if (tr.key !== trackKey) return tr;
        const lanes = tr.lanes.map((lane) =>
          lane.key === laneKey ? { ...lane, monitorNote: note } : lane,
        );
        const next = { ...tr, lanes };
        audioEngine.setLaneCcMidi(
          trackKey,
          laneKey,
          monitorMidiNote(keyPc, note),
        );
        return next;
      });
      return { tracks };
    });
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
    hostClock.stop();
    audioEngine.panic();
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
      audioEngine.muteAll();
      return {
        playing: false,
        transportRunning: false,
        tracks,
        notice:
          "Panic — monitor muted, MIDI Stop + All Notes Off. Host echo paused. Space = unmute.",
      };
    });
  },

  panicTrack: (key) => {
    const tr = get().tracks.find((t) => t.key === key);
    if (!tr) return;
    const channels = [
      ...tr.track.midi.outChannels,
      tr.track.midi.channel,
    ];
    audioEngine.panicTrack(key);
    for (const lane of tr.lanes) {
      if (lane.role === "out") lane.ring.clear();
    }
    if (snapshot && !get().demo) {
      sendMidiPanicChannels(snapshot.device.performanceOutputs, channels);
    }
    set((s) => ({
      tracks: s.tracks.map((t) =>
        t.key === key
          ? { ...t, activity: 0, lastEvent: null, lanes: [...t.lanes] }
          : t,
      ),
      notice: `${tr.track.app.name} — All Notes Off on CH ${[...new Set(channels)].join("/")}`,
    }));
  },

  transportStart: async () => {
    if (get().demo || !snapshot) {
      set({
        transportRunning: true,
        notice: get().demo ? "Demo has no device transport" : "Not connected",
      });
      return;
    }
    try {
      const ensured = await ensureMidiUsbClockSource(snapshot);
      const bpm = clampBpm(ensured?.bpm ?? get().clockBpm);
      hostClock.setOutputs(snapshot.device.performanceOutputs);
      hostClock.setBpm(bpm);
      hostClock.start();
      // Also poke Start on outputs (hostClock already sends 0xFA)
      sendMidiTransport(snapshot.device.performanceOutputs, "start");
      const switched = ensured?.changed
        ? `Clock Src → MIDI USB. `
        : "";
      set({
        transportRunning: true,
        clockSrc: "MidiUsb",
        clockBpm: bpm,
        playing: true,
        notice: `${switched}Host clock + Start @ ${bpm} BPM. Apps need MidiOut→USB for scopes.`,
      });
      audioEngine.setPlaying(true);
      // Unmute if everything was muted after panic — otherwise user hears nothing
      const tracks = get().tracks;
      if (tracks.length > 0 && tracks.every((tr) => tr.muted)) {
        const next = tracks.map((tr) => ({ ...tr, muted: false }));
        for (const tr of next) {
          audioEngine.setTrackState(tr.key, { muted: false, solo: tr.solo });
        }
        set({ tracks: next });
      }
    } catch (err) {
      set({
        notice: null,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  transportStop: () => {
    hostClock.stop();
    if (snapshot && !get().demo) {
      sendMidiTransport(snapshot.device.performanceOutputs, "stop");
    }
    set({
      transportRunning: false,
      notice: get().demo ? "Demo has no device transport" : "Host clock + MIDI Stop sent",
    });
  },

  refreshParams: async () => {
    if (!snapshot || get().demo) return;
    try {
      const updated = await withConfigLock(() => reloadTracks(snapshot!));
      snapshot = { ...snapshot, tracks: updated };
      const usb = countUsbEnabled(updated);
      const { runtimes, collisions } = buildTrackRuntimes(
        updated,
        get().keyPc,
        get().tracks,
      );
      set({
        tracks: runtimes,
        collisions,
        collisionsBannerDismissed: false,
        usbOn: usb.on,
        usbCapable: usb.capable,
        notice:
          collisionNotice(collisions) ??
          (usb.capable > 0 && usb.on === 0
            ? "No app has MidiOut→USB enabled — scopes stay flat until you enable it."
            : "Layout + params refreshed"),
      });
    } catch (err) {
      set({
        notice: err instanceof Error ? err.message : String(err),
      });
    }
  },

  enableUsbMidi: async () => {
    if (!snapshot || get().demo) return;
    set({ notice: "Enabling MidiOut→USB on apps…" });
    try {
      const { usbFix, changed, updated } = await withConfigLock(async () => {
        const usbFix = await ensureUsbOutputLocal(snapshot!);
        const changed = await enableUsbMidiOnAll(snapshot!);
        const updated = await reloadTracks(snapshot!);
        snapshot = { ...snapshot!, tracks: updated };
        return { usbFix, changed, updated };
      });
      const usb = countUsbEnabled(updated);
      const { runtimes, collisions } = buildTrackRuntimes(
        updated,
        get().keyPc,
        get().tracks,
      );
      set({
        tracks: runtimes,
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
      const changed = await withConfigLock(async () => {
        const n = await assignUniqueMidiChannels(snapshot!);
        const updated = await reloadTracks(snapshot!);
        snapshot = { ...snapshot!, tracks: updated };
        return n;
      });
      const updated = snapshot!.tracks;
      const { runtimes, collisions } = buildTrackRuntimes(
        updated,
        get().keyPc,
        get().tracks,
      );
      set((s) => ({
        tracks: runtimes,
        collisions,
        collisionsBannerDismissed:
          collisions.length === 0 ? false : s.collisionsBannerDismissed,
        notice: [
          changed > 0
            ? `Split ${changed} colliding app(s) onto unique MIDI channels.`
            : "No colliding apps to split (or no free channels / AppState read failed).",
          collisionNotice(collisions),
          "If the Configurator was open, reconnect it — it shares the config MIDI cable.",
        ]
          .filter(Boolean)
          .join(" "),
      }));
    } catch (err) {
      set({
        notice: null,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  dismissCollisionsBanner: () => set({ collisionsBannerDismissed: true }),

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

    // Record on every matching Out lane. Audio: unique attribution, else the
    // focused track if it matches, else the first match (shared wire still audible).
    const audioMatches = !ambiguous
      ? matches
      : matches.some((m) => m.key === state.focusKey)
        ? matches.filter((m) => m.key === state.focusKey)
        : matches.slice(0, 1);
    const audioKeys = new Set(audioMatches.map((m) => m.key));
    for (const match of matches) {
      const lane = outLaneForChannel(match.lanes, ev.channel);
      if (lane) pushVoiceToRing(lane.ring, ev);
      if (audioKeys.has(match.key)) {
        audioEngine.handle(match.key, ev, false, lane?.key);
      }
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
