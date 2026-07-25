import type { Color, Param, Value } from "@atov/fp-config";

import {
  connectDevice,
  receiveBatchMessages,
  sendAndReceive,
  type DeviceBundle,
} from "../midi/device";

export interface AppMeta {
  appId: number;
  channels: number;
  name: string;
  description: string;
  color: Color["tag"] | string;
  params: Param[];
}

export interface TrackMidi {
  /** MidiOut → USB enabled */
  usbEnabled: boolean;
  /** Primary out channel (wire identity for scopes). */
  channel: number; // 1–16
  /** All MidiOut channels (Kick/Snare/Hats, Out A/Pong, …). */
  outChannels: number[];
  /** MidiIn channel when the app has MidiIn + a following MidiChannel. */
  inChannel: number | null;
  /** MidiIn → USB enabled (null = no MidiIn param). */
  inUsb: boolean | null;
  cc: number | null; // 0–127 when CC app
  /**
   * Primary monitor is notes (MIDI pitch). False = CC envelope at Wave-Hz.
   * Hybrid apps (note + CC without exclusive MidiMode) keep this true and set playCc.
   */
  noteMode: boolean;
  /** Accept CC/NRPN for scope + CC-Hz voice (Heat Pump, or note+CC hybrids). */
  playCc: boolean;
  /** Configured MidiNote values (Kick/Snare/… setup) — for labels / sanity. */
  setupNotes: number[];
  nrpn: boolean;
}

export interface AppTrack {
  key: string;
  layoutId: number;
  startChannel: number; // physical fader index 0–15
  width: number;
  app: AppMeta;
  midi: TrackMidi;
  hasMidiMirror: boolean;
}

export interface Snapshot {
  version: string;
  apps: Map<number, AppMeta>;
  tracks: AppTrack[];
  device: DeviceBundle;
}

function colorTag(color: Color): string {
  return color.tag === "Custom" ? "Custom" : color.tag;
}

function midiChannelFromValue(value: Value | undefined): number {
  if (value?.tag === "MidiChannel") return Math.max(1, Math.min(16, value.value[0]));
  return 1;
}

function midiCcFromValue(value: Value | undefined): number | null {
  if (value?.tag === "MidiCc") return Math.max(0, Math.min(127, value.value[0] & 0x7f));
  return null;
}

function midiOutUsb(value: Value | undefined): boolean {
  if (value?.tag === "MidiOut") return Boolean(value.value[0][0]);
  return false;
}

function midiModeNote(value: Value | undefined): boolean | null {
  if (value?.tag === "MidiMode") return value.value.tag === "Note";
  return null;
}

function midiNoteFromValue(value: Value | undefined): number | null {
  if (value?.tag === "MidiNote") return Math.max(0, Math.min(127, value.value[0] & 0x7f));
  return null;
}

function midiNrpn(value: Value | undefined): boolean {
  return value?.tag === "MidiNrpn" ? value.value : false;
}

function midiInUsb(value: Value | undefined): boolean {
  if (value?.tag === "MidiIn") return Boolean(value.value[0][0]);
  return false;
}

/** Golden Gate / Heat Pump / MIDI→CV style: Enum "Mode" with Note|CC|Pitch|… */
function inferExclusiveModeFromEnum(params: Param[], values: Value[]): boolean | null {
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    if (param.tag !== "Enum") continue;
    if (!/^mode$/i.test(param.value.name)) continue;
    const variants = param.value.variants;
    const raw = values[i];
    if (raw?.tag !== "Enum") continue;
    const idx = Number(raw.value);
    const label = variants[idx] ?? "";
    if (/^(note|pitch|phi|gate)/i.test(label)) return true;
    if (/^cc$/i.test(label)) return false;
  }
  return null;
}

/** Sequencer / drum / clock / generative note apps — notes are the musical output. */
function nameSuggestsNotes(appName: string): boolean {
  return /seq|euclid|turing|grids|groove|tb3|bernoulli|trigger|note|clk|gate|echo|arp|vamp|l[eé]vy/i.test(
    appName,
  );
}

/**
 * Decide note vs CC monitor.
 * Explicit MidiMode / Enum Mode wins over name heuristics.
 * When an app exposes both MidiNote and MidiCc without an exclusive mode,
 * prefer notes (pitch from setup) and still accept CC as secondary (hybrid).
 */
function inferMonitorFlags(
  params: Param[],
  values: Value[],
  appName: string,
): { noteMode: boolean; playCc: boolean } {
  const modeIdx = params.findIndex((p) => p.tag === "MidiMode");
  if (modeIdx >= 0) {
    const explicit = midiModeNote(values[modeIdx]);
    if (explicit !== null) {
      return { noteMode: explicit, playCc: !explicit };
    }
  }
  const fromEnum = inferExclusiveModeFromEnum(params, values);
  if (fromEnum !== null) {
    return { noteMode: fromEnum, playCc: !fromEnum };
  }

  const hasCc = params.some((p) => p.tag === "MidiCc");
  const hasNote = params.some((p) => p.tag === "MidiNote");
  const hasMidiIn = params.some((p) => p.tag === "MidiIn");

  if (hasNote && hasCc) {
    // Dual output (or dual-capable): notes are the musical event; CC is secondary.
    return { noteMode: true, playCc: true };
  }
  if (hasMidiIn && hasNote) return { noteMode: true, playCc: false };
  if (hasNote) return { noteMode: true, playCc: false };
  if (hasCc) return { noteMode: false, playCc: true };
  if (nameSuggestsNotes(appName)) return { noteMode: true, playCc: false };
  return { noteMode: false, playCc: true };
}

/**
 * Parse MIDI I/O from CONFIG order:
 * - MidiChannel after MidiIn → input channel
 * - MidiChannel after MidiOut → output channel(s)
 * - MidiChannel(s) before MidiOut (or lone) → all are output channels
 *   (Grooves Kick/Snare/Hats, FP Grids, …)
 */
function extractMidi(params: Param[], values: Value[], appName: string): TrackMidi {
  let outChannel = 1;
  let outChannels: number[] = [];
  let inChannel: number | null = null;
  let inUsb: boolean | null = null;
  let cc: number | null = null;
  let usbEnabled = false;
  let nrpn = false;
  let sawMidiIn = false;
  let outChannelSet = false;
  const setupNotes: number[] = [];

  params.forEach((param, i) => {
    const value = values[i];
    switch (param.tag) {
      case "MidiIn":
        sawMidiIn = true;
        inUsb = midiInUsb(value);
        break;
      case "MidiOut":
        usbEnabled = midiOutUsb(value);
        break;
      case "MidiChannel": {
        const ch = midiChannelFromValue(value);
        // Channel between MidiIn and first outs = input (before any out channel seen)
        if (sawMidiIn && inChannel === null && !outChannelSet) {
          inChannel = ch;
        } else {
          // All other MidiChannels are outs (may appear before or after MidiOut)
          outChannels.push(ch);
          if (!outChannelSet) {
            outChannel = ch;
            outChannelSet = true;
          }
        }
        break;
      }
      case "MidiCc":
        cc = midiCcFromValue(value);
        break;
      case "MidiNote": {
        const n = midiNoteFromValue(value);
        if (n !== null) setupNotes.push(n);
        break;
      }
      case "MidiNrpn":
        nrpn = midiNrpn(value);
        break;
      default:
        break;
    }
  });

  // No MidiIn in CONFIG: single MidiChannel is out (classic apps)
  if (!sawMidiIn && outChannelSet) {
    inChannel = null;
    inUsb = null;
  }
  if (outChannels.length === 0) outChannels = [outChannel];
  // Dedupe while preserving order (same channel on Kick/Snare defaults)
  outChannels = [...new Set(outChannels)];

  const { noteMode, playCc } = inferMonitorFlags(params, values, appName);

  return {
    usbEnabled,
    channel: outChannel,
    outChannels,
    inChannel,
    inUsb,
    cc,
    noteMode,
    playCc,
    setupNotes,
    nrpn,
  };
}

export async function loadSnapshot(): Promise<Snapshot> {
  const device = await connectDevice();
  const { config } = device;

  const appsResponse = await sendAndReceive(config, { tag: "GetAllApps" });
  if (appsResponse.tag !== "BatchMsgStart") {
    throw new Error(`GetAllApps failed: ${appsResponse.tag}`);
  }
  const appMsgs = await receiveBatchMessages(config, appsResponse.value);
  const apps = new Map<number, AppMeta>();
  for (const item of appMsgs) {
    if (item.tag !== "AppConfig") continue;
    const [appId, channels, meta] = item.value;
    apps.set(appId, {
      appId,
      channels: Number(channels),
      name: meta[1],
      description: meta[2],
      color: colorTag(meta[3]),
      params: meta[5],
    });
  }

  const layoutResponse = await sendAndReceive(config, { tag: "GetLayout" });
  if (layoutResponse.tag !== "Layout") {
    throw new Error(`GetLayout failed: ${layoutResponse.tag}`);
  }
  const layoutSlots = layoutResponse.value[0];

  const tracks: AppTrack[] = [];
  for (let startChannel = 0; startChannel < 16; startChannel++) {
    const slot = layoutSlots[startChannel];
    if (!slot) continue;
    const [appId, widthBig, layoutId] = slot;
    const app = apps.get(appId);
    if (!app) continue;

    const paramsResponse = await sendAndReceive(config, {
      tag: "GetAppParams",
      value: { layout_id: layoutId },
    });
    if (paramsResponse.tag !== "AppState") continue;
    const values = paramsResponse.value[1];
    const midi = extractMidi(app.params, values, app.name);
    const hasMidiMirror =
      app.params.some((p) => p.tag === "MidiOut" || p.tag === "MidiChannel") &&
      !/midi→cv|midi->cv|offset|slew|follower|quantizer|^ad$/i.test(app.name);

    tracks.push({
      key: `${layoutId}-${appId}-${startChannel}`,
      layoutId,
      startChannel,
      width: Number(widthBig),
      app,
      midi,
      hasMidiMirror,
    });
  }

  return {
    version: config.version,
    apps,
    tracks,
    device,
  };
}

export async function refreshTrackParams(snapshot: Snapshot): Promise<AppTrack[]> {
  const { config } = snapshot.device;
  const next: AppTrack[] = [];
  for (const track of snapshot.tracks) {
    const paramsResponse = await sendAndReceive(config, {
      tag: "GetAppParams",
      value: { layout_id: track.layoutId },
    });
    if (paramsResponse.tag !== "AppState") {
      next.push(track);
      continue;
    }
    const midi = extractMidi(track.app.params, paramsResponse.value[1], track.app.name);
    next.push({ ...track, midi });
  }
  return next;
}

function padParams(values: Value[]): import("@atov/fp-config").FixedLengthArray<Value | undefined, 16> {
  const result: (Value | undefined)[] = Array.from({ length: 16 }, () => undefined);
  values.forEach((v, i) => {
    if (i < 16) result[i] = v;
  });
  return result as unknown as import("@atov/fp-config").FixedLengthArray<Value | undefined, 16>;
}

/** Turn on MidiOut→USB for every layout app that has a MidiOut param. */
export async function enableUsbMidiOnAll(snapshot: Snapshot): Promise<number> {
  const { config } = snapshot.device;
  let changed = 0;

  for (const track of snapshot.tracks) {
    const midiOutIdx = track.app.params.findIndex((p) => p.tag === "MidiOut");
    if (midiOutIdx < 0) continue;

    const paramsResponse = await sendAndReceive(config, {
      tag: "GetAppParams",
      value: { layout_id: track.layoutId },
    });
    if (paramsResponse.tag !== "AppState") continue;
    const values = [...paramsResponse.value[1]];
    while (values.length <= midiOutIdx) values.push({ tag: "bool", value: false });

    const current = values[midiOutIdx];
    let out1 = false;
    let out2 = false;
    if (current?.tag === "MidiOut") {
      out1 = Boolean(current.value[0][1]);
      out2 = Boolean(current.value[0][2]);
      if (current.value[0][0]) continue; // already on
    }
    values[midiOutIdx] = { tag: "MidiOut", value: [[true, out1, out2]] };
    await sendAndReceive(config, {
      tag: "SetAppParams",
      value: { layout_id: track.layoutId, values: padParams(values) },
    });
    changed++;
  }
  return changed;
}

/** Ensure USB MIDI port mode is Local so app mirrors can leave the device. */
export async function ensureUsbOutputLocal(snapshot: Snapshot): Promise<string | null> {
  const { config } = snapshot.device;
  const response = await sendAndReceive(config, { tag: "GetGlobalConfig" });
  if (response.tag !== "GlobalConfig") return null;
  const gc = response.value;
  const usb = gc.midi.outs[0];
  const needsMode = usb.mode.tag !== "Local";
  const needsClock = !usb.send_clock || !usb.send_transport;
  if (!needsMode && !needsClock) return null;

  const outs = [gc.midi.outs[0], gc.midi.outs[1], gc.midi.outs[2]] as unknown as typeof gc.midi.outs;
  outs[0] = {
    send_clock: true,
    send_transport: true,
    mode: { tag: "Local" },
  };
  await sendAndReceive(config, {
    tag: "SetGlobalConfig",
    value: { ...gc, midi: { outs } },
  });
  const parts: string[] = [];
  if (needsMode) parts.push(`USB MIDI mode ${usb.mode.tag} → Local`);
  if (needsClock) parts.push("USB send clock/transport on");
  return parts.join("; ");
}

export type ClockSrcTag = string;

export async function readClockConfig(
  snapshot: Snapshot,
): Promise<{ src: ClockSrcTag; bpm: number } | null> {
  const response = await sendAndReceive(snapshot.device.config, { tag: "GetGlobalConfig" });
  if (response.tag !== "GlobalConfig") return null;
  const clock = response.value.clock;
  return {
    src: clock.clock_src.tag,
    bpm: Math.max(20, Math.min(300, Number(clock.internal_bpm) || 120)),
  };
}

/**
 * Point the device at MIDI USB clock so the diagnostics host can Start/Stop + tick.
 * Keeps the configured internal BPM as the host tempo reference.
 */
export async function ensureMidiUsbClockSource(
  snapshot: Snapshot,
): Promise<{ src: ClockSrcTag; bpm: number; changed: boolean } | null> {
  const response = await sendAndReceive(snapshot.device.config, { tag: "GetGlobalConfig" });
  if (response.tag !== "GlobalConfig") return null;
  const gc = response.value;
  const bpm = Math.max(20, Math.min(300, Number(gc.clock.internal_bpm) || 120));
  if (gc.clock.clock_src.tag === "MidiUsb") {
    return { src: "MidiUsb", bpm, changed: false };
  }
  await sendAndReceive(snapshot.device.config, {
    tag: "SetGlobalConfig",
    value: {
      ...gc,
      clock: {
        ...gc.clock,
        clock_src: { tag: "MidiUsb" },
      },
    },
  });
  return { src: "MidiUsb", bpm, changed: true };
}

export function countUsbEnabled(tracks: AppTrack[]): { on: number; capable: number } {
  let on = 0;
  let capable = 0;
  for (const t of tracks) {
    if (!t.hasMidiMirror) continue;
    capable++;
    if (t.midi.usbEnabled) on++;
  }
  return { on, capable };
}

/** Identity used for MIDI attribution (no app id on the wire). */
export function midiIdentityKey(track: AppTrack): string {
  // Note-primary apps collide on channel notes (even if they also emit CC)
  if (track.midi.noteMode) {
    const chs = track.midi.outChannels.slice().sort((a, b) => a - b).join(",");
    return `ch${chs}:notes`;
  }
  if (track.midi.cc !== null) {
    return `ch${track.midi.channel}:cc${track.midi.cc}${track.midi.nrpn ? ":nrpn" : ""}`;
  }
  return `ch${track.midi.channel}:cc*`;
}

export interface MidiCollision {
  key: string;
  trackKeys: string[];
  label: string;
}

export function findMidiCollisions(tracks: AppTrack[]): MidiCollision[] {
  const capable = tracks.filter((t) => t.hasMidiMirror);
  const groups = new Map<string, AppTrack[]>();
  for (const t of capable) {
    const key = midiIdentityKey(t);
    const list = groups.get(key) ?? [];
    list.push(t);
    groups.set(key, list);
  }
  const out: MidiCollision[] = [];
  for (const [key, list] of groups) {
    if (list.length < 2) continue;
    out.push({
      key,
      trackKeys: list.map((t) => t.key),
      label: list.map((t) => t.app.name).join(" + "),
    });
  }
  return out;
}

/**
 * Assign distinct MIDI channels so colliding apps no longer share a wire identity.
 * Sparse SetAppParams (only MidiChannel slots) — avoids rewriting every param and
 * reduces risk of confusing a simultaneously open Configurator on the config cable.
 */
export async function assignUniqueMidiChannels(snapshot: Snapshot): Promise<number> {
  const { config } = snapshot.device;
  const collisions = findMidiCollisions(snapshot.tracks);
  if (collisions.length === 0) return 0;

  const collidingKeys = new Set(collisions.flatMap((c) => c.trackKeys));
  const used = new Set<number>();

  // Preserve channels already unique (non-colliding apps keep theirs)
  for (const track of snapshot.tracks) {
    if (collidingKeys.has(track.key)) continue;
    if (track.app.params.some((p) => p.tag === "MidiChannel")) {
      used.add(track.midi.channel);
    }
  }

  let nextCh = 1;
  const alloc = (): number | null => {
    while (nextCh <= 16 && used.has(nextCh)) nextCh++;
    if (nextCh > 16) return null;
    const ch = nextCh;
    used.add(ch);
    nextCh++;
    return ch;
  };

  let changed = 0;

  for (const track of snapshot.tracks) {
    if (!collidingKeys.has(track.key)) continue;

    const paramsResponse = await sendAndReceive(config, {
      tag: "GetAppParams",
      value: { layout_id: track.layoutId },
    });
    if (paramsResponse.tag !== "AppState") continue;
    const values = paramsResponse.value[1];

    // Prefer index from live values (what FRAM/app actually holds), fall back to CONFIG order
    let chIdx = values.findIndex((v) => v?.tag === "MidiChannel");
    if (chIdx < 0) {
      chIdx = track.app.params.findIndex((p) => p.tag === "MidiChannel");
    }
    if (chIdx < 0) continue;

    const ch = alloc();
    if (ch === null) break;

    const currentVal = values[chIdx];
    const current =
      currentVal?.tag === "MidiChannel" ? currentVal.value[0] : null;
    if (current === ch) continue;

    // Sparse write: only MidiChannel — firmware merges into existing params
    const sparse = padParams([]);
    sparse[chIdx] = { tag: "MidiChannel", value: [ch] };

    const setResponse = await sendAndReceive(config, {
      tag: "SetAppParams",
      value: { layout_id: track.layoutId, values: sparse },
    });
    if (setResponse.tag !== "AppState") {
      throw new Error(
        `SetAppParams failed for ${track.app.name}: got ${setResponse.tag}`,
      );
    }
    changed++;
  }
  return changed;
}
