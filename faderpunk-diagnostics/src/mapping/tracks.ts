import type { Color, Param, Value } from "@atov/fp-config";

import {
  connectDevice,
  drainConfigQueue,
  receiveBatchMessages,
  sendAndReceive,
  type ConfigPort,
  type DeviceBundle,
} from "../midi/device";

export interface AppMeta {
  appId: number;
  channels: number;
  name: string;
  description: string;
  color: Color["tag"] | string;
  icon: string;
  params: Param[];
}

export interface TrackMidi {
  /** MidiOut → USB enabled */
  usbEnabled: boolean;
  /** MidiOut → DIN Out 1 / Out 2 (null = no MidiOut param). */
  out1: boolean | null;
  out2: boolean | null;
  /** Primary out channel (wire identity for scopes). */
  channel: number; // 1–16
  /** All MidiOut channels (Kick/Snare/Hats, Out A/Pong, …). */
  outChannels: number[];
  /** Param names aligned with outChannels (Kick, Snare, …). */
  outChannelNames: string[];
  /** MidiIn channel when the app has MidiIn + a following MidiChannel. */
  inChannel: number | null;
  /** MidiIn → USB enabled (null = no MidiIn param). */
  inUsb: boolean | null;
  /** MidiIn → DIN enabled (null = no MidiIn param). */
  inDin: boolean | null;
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

/** One live device param, formatted for slot readout. */
export interface ParamRow {
  name: string;
  kind: string;
  text: string;
}

export interface AppTrack {
  key: string;
  layoutId: number;
  startChannel: number; // physical fader index 0–15
  width: number;
  app: AppMeta;
  midi: TrackMidi;
  /** All live AppState values (GetAppParams), name + formatted text. */
  paramRows: ParamRow[];
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

/** Protocol ints may arrive as number | bigint — always narrow to u8-ish number. */
function asU8(n: unknown): number {
  const v = typeof n === "bigint" ? Number(n) : Number(n);
  if (!Number.isFinite(v)) return 0;
  return Math.max(0, Math.min(255, Math.round(v)));
}

function midiChannelFromValue(value: Value | undefined): number {
  if (value?.tag !== "MidiChannel") return 1;
  const raw = value.value;
  const n = Number(Array.isArray(raw) ? raw[0] : raw);
  if (!Number.isFinite(n)) return 1;
  return Math.max(1, Math.min(16, Math.round(n)));
}

function midiCcFromValue(value: Value | undefined): number | null {
  if (value?.tag !== "MidiCc") return null;
  const n = Number(Array.isArray(value.value) ? value.value[0] : value.value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(127, Math.round(n) & 0x7f));
}

function midiOutFlags(
  value: Value | undefined,
): { usb: boolean; out1: boolean; out2: boolean } | null {
  if (value?.tag !== "MidiOut") return null;
  const raw = value.value;
  const flags = (Array.isArray(raw[0]) ? raw[0] : raw) as boolean[];
  return {
    usb: Boolean(flags?.[0]),
    out1: Boolean(flags?.[1]),
    out2: Boolean(flags?.[2]),
  };
}

function midiModeNote(value: Value | undefined): boolean | null {
  if (value?.tag === "MidiMode") return value.value.tag === "Note";
  return null;
}

function midiNoteFromValue(value: Value | undefined): number | null {
  if (value?.tag !== "MidiNote") return null;
  const n = Number(Array.isArray(value.value) ? value.value[0] : value.value);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(127, Math.round(n) & 0x7f));
}

function midiNrpn(value: Value | undefined): boolean {
  return value?.tag === "MidiNrpn" ? value.value : false;
}

function midiInFlags(
  value: Value | undefined,
): { usb: boolean; din: boolean } | null {
  if (value?.tag !== "MidiIn") return null;
  const raw = value.value;
  const flags = (Array.isArray(raw[0]) ? raw[0] : raw) as boolean[];
  return { usb: Boolean(flags?.[0]), din: Boolean(flags?.[1]) };
}

function enumTag(v: unknown): string {
  if (v && typeof v === "object" && "tag" in v) {
    return String((v as { tag: string }).tag);
  }
  return String(v);
}

function rangeLabel(tag: string): string {
  switch (tag) {
    case "_0_10V":
      return "0–10V";
    case "_0_5V":
      return "0–5V";
    case "_Neg5_5V":
      return "±5V";
    default:
      return tag;
  }
}

function noteLabel(tag: string): string {
  const map: Record<string, string> = {
    C: "C",
    CSharp: "C♯",
    D: "D",
    DSharp: "D♯",
    E: "E",
    F: "F",
    FSharp: "F♯",
    G: "G",
    GSharp: "G♯",
    A: "A",
    ASharp: "A♯",
    B: "B",
  };
  return map[tag] ?? tag;
}

function paramName(param: Param): string {
  switch (param.tag) {
    case "None":
      return "—";
    case "MidiIn":
      return "MidiIn";
    case "MidiOut":
      return "MidiOut";
    case "MidiMode":
      return "MidiMode";
    case "MidiNrpn":
      return "NRPN";
    case "VoltPerOct":
      return "V/Oct";
    case "i32":
    case "f32":
    case "bool":
    case "Enum":
    case "Curve":
    case "Waveform":
    case "Color":
    case "Range":
    case "Note":
    case "MidiCc":
    case "MidiChannel":
    case "MidiNote":
      return param.value.name;
  }
}

/** Format one live Value against its Param schema for slot readout. */
export function formatParamRow(param: Param, value: Value | undefined): ParamRow | null {
  if (param.tag === "None") return null;
  const name = paramName(param);
  const kind = param.tag;

  if (!value) {
    return { name, kind, text: "—" };
  }

  switch (value.tag) {
    case "i32":
    case "f32": {
      const n = Number(Array.isArray(value.value) ? value.value[0] : value.value);
      return {
        name,
        kind,
        text: Number.isFinite(n)
          ? value.tag === "f32"
            ? String(Math.round(n * 1000) / 1000)
            : String(Math.round(n))
          : "—",
      };
    }
    case "bool":
      return { name, kind, text: value.value ? "on" : "off" };
    case "Enum": {
      const idx = Number(value.value);
      const variants =
        param.tag === "Enum" ? param.value.variants : undefined;
      const label =
        variants && Number.isFinite(idx) ? (variants[idx] ?? `#${idx}`) : String(idx);
      return { name, kind, text: label };
    }
    case "Curve":
    case "Waveform":
    case "VoltPerOct":
      return { name, kind, text: enumTag(value.value) };
    case "Range":
      return { name, kind, text: rangeLabel(enumTag(value.value)) };
    case "Note":
      return { name, kind, text: noteLabel(enumTag(value.value)) };
    case "Color": {
      const c = value.value;
      if (c.tag === "Custom") {
        const [r, g, b] = c.value;
        return { name, kind, text: `rgb(${asU8(r)},${asU8(g)},${asU8(b)})` };
      }
      return { name, kind, text: c.tag };
    }
    case "MidiCc": {
      const n = midiCcFromValue(value);
      return { name, kind, text: n !== null ? String(n) : "—" };
    }
    case "MidiChannel":
      return { name, kind, text: String(midiChannelFromValue(value)) };
    case "MidiIn": {
      const f = midiInFlags(value);
      if (!f) return { name, kind, text: "—" };
      const parts: string[] = [];
      if (f.usb) parts.push("USB");
      if (f.din) parts.push("DIN");
      return { name, kind, text: parts.length ? parts.join("+") : "off" };
    }
    case "MidiOut": {
      const f = midiOutFlags(value);
      if (!f) return { name, kind, text: "—" };
      const parts: string[] = [];
      if (f.usb) parts.push("USB");
      if (f.out1) parts.push("Out1");
      if (f.out2) parts.push("Out2");
      return { name, kind, text: parts.length ? parts.join("+") : "off" };
    }
    case "MidiMode":
      return { name, kind, text: value.value.tag };
    case "MidiNote": {
      const n = midiNoteFromValue(value);
      return { name, kind, text: n !== null ? String(n) : "—" };
    }
    case "MidiNrpn":
      return { name, kind, text: value.value ? "on" : "off" };
  }
}

function buildParamRows(params: Param[], values: Value[]): ParamRow[] {
  const rows: ParamRow[] = [];
  const n = Math.max(params.length, values.length);
  for (let i = 0; i < n; i++) {
    const param = params[i];
    if (!param || param.tag === "None") continue;
    const row = formatParamRow(param, values[i]);
    if (row) rows.push(row);
  }
  return rows;
}

/** Patch one track from a live AppState values payload (push or soft poll). */
export function applyAppStateToTrack(
  track: AppTrack,
  values: Value[],
  apps?: Map<number, AppMeta>,
): AppTrack {
  if (!values.length) return track;
  const schema = apps?.get(track.app.appId) ?? track.app;
  const app = withLiveColor(schema, values);
  const paramRows = buildParamRows(schema.params, values);
  const hasMidiMirror =
    schema.params.some((p) => p.tag === "MidiOut" || p.tag === "MidiChannel") &&
    !/midi→cv|midi->cv|offset|slew|follower|quantizer|^ad$/i.test(schema.name);

  // Only rewrite wire identity when every MidiChannel slot is present — never
  // invent CH1 from an incomplete/mis-paired AppState.
  if (midiChannelValuesPresent(schema.params, values)) {
    const midi = extractMidi(schema.params, values, schema.name);
    return { ...track, app, midi, paramRows, hasMidiMirror };
  }

  return {
    ...track,
    app,
    paramRows,
    hasMidiMirror,
    midi: {
      ...track.midi,
      ...monitorFlagsOnly(schema.params, values, schema.name, track.midi),
    },
  };
}

/** True when every MidiChannel CONFIG slot has a matching live Value. */
function midiChannelValuesPresent(params: Param[], values: Value[]): boolean {
  let saw = false;
  for (let i = 0; i < params.length; i++) {
    if (params[i]?.tag !== "MidiChannel") continue;
    saw = true;
    if (values[i]?.tag !== "MidiChannel") return false;
  }
  return saw;
}

function monitorFlagsOnly(
  params: Param[],
  values: Value[],
  appName: string,
  prior: TrackMidi,
): Pick<TrackMidi, "noteMode" | "playCc" | "cc"> {
  const { noteMode, playCc } = inferMonitorFlags(params, values, appName);
  let cc = prior.cc;
  for (let i = 0; i < params.length; i++) {
    if (params[i]?.tag !== "MidiCc") continue;
    const next = midiCcFromValue(values[i]);
    if (next !== null) cc = next;
  }
  return { noteMode, playCc, cc };
}

/** Live Color param (overrides static CONFIG color from AppConfig). */
function colorFromValue(v: Value | undefined): string | null {
  if (v?.tag !== "Color") return null;
  if (v.value.tag === "Custom") {
    const [r, g, b] = v.value.value;
    return `rgb(${asU8(r)},${asU8(g)},${asU8(b)})`;
  }
  return colorTag(v.value);
}

function colorFromValues(params: Param[], values: Value[]): string | null {
  for (let i = 0; i < params.length; i++) {
    if (params[i]?.tag !== "Color") continue;
    const live = colorFromValue(values[i]);
    if (live) return live;
  }
  // Params/values index drift (legacy layouts) — still prefer any Color in the payload.
  for (const v of values) {
    const live = colorFromValue(v);
    if (live) return live;
  }
  return null;
}

function withLiveColor(app: AppMeta, values: Value[]): AppMeta {
  const live = colorFromValues(app.params, values);
  return live ? { ...app, color: live } : app;
}

async function getAppParams(
  config: ConfigPort,
  layoutIdRaw: unknown,
): Promise<Value[] | null> {
  const layoutId = asU8(layoutIdRaw);
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      if (config.rx.waiter) {
        // Another config op in flight — wait briefly rather than collide.
        await new Promise((r) => setTimeout(r, 40 + attempt * 40));
      }
      drainConfigQueue(config.rx);
      const paramsResponse = await sendAndReceive(config, {
        tag: "GetAppParams",
        value: { layout_id: layoutId },
      });
      if (paramsResponse.tag !== "AppState") {
        if (attempt < 2) continue;
        return null;
      }
      if (asU8(paramsResponse.value[0]) !== layoutId) {
        console.warn(
          `AppState layout_id mismatch: requested ${layoutId}, got ${paramsResponse.value[0]}`,
        );
        if (attempt < 2) continue;
        return null;
      }
      const values = paramsResponse.value[1];
      if (!values.length) {
        if (attempt < 2) continue;
        return null;
      }
      return values;
    } catch (err) {
      console.warn(`GetAppParams layout ${layoutId} attempt ${attempt + 1}:`, err);
      if (attempt === 2) return null;
      await new Promise((r) => setTimeout(r, 60 + attempt * 40));
    }
  }
  return null;
}

function countLayoutSlots(
  layoutSlots: ([number, bigint, number] | undefined)[] | readonly ([number, bigint, number] | undefined)[],
): number {
  let n = 0;
  let lastUsed = -1;
  for (let startChannel = 0; startChannel < 16; startChannel++) {
    if (startChannel <= lastUsed) continue;
    const slot = layoutSlots[startChannel];
    if (!slot) {
      lastUsed = startChannel;
      continue;
    }
    const width = Math.max(1, Number(slot[1]) || 1);
    lastUsed = startChannel + width - 1;
    n++;
  }
  return n;
}

function buildTracksFromLayout(
  layoutSlots: ([number, bigint, number] | undefined)[] | readonly ([number, bigint, number] | undefined)[],
  apps: Map<number, AppMeta>,
  paramsByLayout: Map<number, Value[]>,
): AppTrack[] {
  const tracks: AppTrack[] = [];
  let lastUsed = -1;

  for (let startChannel = 0; startChannel < 16; startChannel++) {
    if (startChannel <= lastUsed) continue;
    const slot = layoutSlots[startChannel];
    if (!slot) {
      lastUsed = startChannel;
      continue;
    }
    const appId = asU8(slot[0]);
    const width = Math.max(1, Number(slot[1]) || 1);
    const layoutId = asU8(slot[2]);
    lastUsed = startChannel + width - 1;

    const appBase = apps.get(appId);
    if (!appBase) continue;

    // Still show the slot if GetAppParams timed out / mismatched — better than a blank grid.
    // But never invent CH1 from an empty values array (looks like real FRAM defaults).
    const values = paramsByLayout.get(layoutId);
    if (!values || values.length === 0) {
      console.warn(
        `No live AppState for ${appBase.name} (layout ${layoutId}) — skipping wire MIDI (retry Refresh)`,
      );
      tracks.push({
        key: `${layoutId}-${appId}-${startChannel}`,
        layoutId,
        startChannel,
        width,
        app: appBase,
        midi: {
          usbEnabled: false,
          out1: null,
          out2: null,
          channel: 0,
          outChannels: [],
          outChannelNames: [],
          inChannel: null,
          inUsb: null,
          inDin: null,
          cc: null,
          noteMode: true,
          playCc: false,
          setupNotes: [],
          nrpn: false,
        },
        paramRows: [],
        hasMidiMirror: false,
      });
      continue;
    }

    const app = withLiveColor(appBase, values);
    const midi = extractMidi(app.params, values, app.name);
    const paramRows = buildParamRows(app.params, values);
    const hasMidiMirror =
      app.params.some((p) => p.tag === "MidiOut" || p.tag === "MidiChannel") &&
      !/midi→cv|midi->cv|offset|slew|follower|quantizer|^ad$/i.test(app.name);

    tracks.push({
      key: `${layoutId}-${appId}-${startChannel}`,
      layoutId,
      startChannel,
      width,
      app,
      midi,
      paramRows,
      hasMidiMirror,
    });
  }
  return tracks;
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

/** Read a named Enum param's selected variant label (normalizes →/->). */
function enumVariantLabel(
  params: Param[],
  values: Value[],
  nameRe: RegExp,
): string | null {
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    if (param.tag !== "Enum") continue;
    if (!nameRe.test(param.value.name)) continue;
    const raw = values[i];
    if (raw?.tag !== "Enum") continue;
    const idx = Number(raw.value);
    const label = param.value.variants[idx];
    return label ? label.replace(/→/g, "->") : null;
  }
  return null;
}

/** Echolot-style Signal enum: CV→CC is CC-only; Gate→Note is notes. */
function inferSignalMonitorFlags(
  params: Param[],
  values: Value[],
): { noteMode: boolean; playCc: boolean } | null {
  const signal = enumVariantLabel(params, values, /^signal$/i);
  if (!signal) return null;
  if (/cv\s*->\s*cc/i.test(signal)) return { noteMode: false, playCc: true };
  if (/gate\s*->\s*note/i.test(signal)) return { noteMode: true, playCc: false };
  if (/^pitch$/i.test(signal)) return { noteMode: true, playCc: false };
  if (/^gate$/i.test(signal)) return { noteMode: true, playCc: false };
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
 * Explicit MidiMode / Enum Mode / Signal wins over name heuristics.
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
  const fromSignal = inferSignalMonitorFlags(params, values);
  if (fromSignal !== null) return fromSignal;
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
 * - MidiChannel after MidiIn (before MidiOut) → input channel
 * - MidiChannel after MidiOut → output channel(s) (Ping / Pong / …)
 * - MidiChannel(s) before MidiOut (or lone) → all are output channels
 *   (Grooves Kick/Snare/Hats, FP Grids, …)
 */
function extractMidi(params: Param[], values: Value[], appName: string): TrackMidi {
  let outChannel = 1;
  let outChannels: number[] = [];
  let outChannelNames: string[] = [];
  let inChannel: number | null = null;
  let inUsb: boolean | null = null;
  let inDin: boolean | null = null;
  let cc: number | null = null;
  let usbEnabled = false;
  let out1: boolean | null = null;
  let out2: boolean | null = null;
  let nrpn = false;
  let sawMidiIn = false;
  let sawMidiOut = false;
  let outChannelSet = false;
  const setupNotes: number[] = [];

  params.forEach((param, i) => {
    const value = values[i];
    switch (param.tag) {
      case "MidiIn": {
        sawMidiIn = true;
        const flags = midiInFlags(value);
        inUsb = flags?.usb ?? false;
        inDin = flags?.din ?? false;
        break;
      }
      case "MidiOut": {
        sawMidiOut = true;
        const flags = midiOutFlags(value);
        usbEnabled = flags?.usb ?? false;
        out1 = flags?.out1 ?? false;
        out2 = flags?.out2 ?? false;
        break;
      }
      case "MidiChannel": {
        const ch = midiChannelFromValue(value);
        const pname =
          "name" in param && typeof (param as { name?: unknown }).name === "string"
            ? (param as { name: string }).name
            : "MIDI Out";
        // First MidiChannel after MidiIn (before MidiOut) = input
        if (sawMidiIn && inChannel === null && !sawMidiOut) {
          inChannel = ch;
        } else {
          // Everything else is an out (Ping/Pong after MidiOut, or Kick/Snare before it)
          outChannels.push(ch);
          outChannelNames.push(pname);
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
    inDin = null;
  }
  if (outChannels.length === 0) {
    outChannels = [outChannel];
    outChannelNames = ["MIDI Out"];
  }
  // Dedupe channels while keeping first name
  const seen = new Set<number>();
  const dedupCh: number[] = [];
  const dedupNames: string[] = [];
  for (let i = 0; i < outChannels.length; i++) {
    const ch = outChannels[i];
    if (seen.has(ch)) continue;
    seen.add(ch);
    dedupCh.push(ch);
    dedupNames.push(outChannelNames[i] ?? `MIDI Out ${dedupCh.length}`);
  }
  outChannels = dedupCh;
  outChannelNames = dedupNames;

  let { noteMode, playCc } = inferMonitorFlags(params, values, appName);

  // Echolot-style I/O + Routing: MidiIn / Pong params stay in CONFIG even when
  // the live mode does not use them (firmware gates Ping-Pong to MIDI→MIDI).
  const io = enumVariantLabel(params, values, /^i\/?o$/i);
  const routing = enumVariantLabel(params, values, /^routing$/i);
  const midiToMidi = io !== null && /midi\s*->\s*midi/i.test(io);
  const cvToMidi = io !== null && /cv\s*->\s*midi/i.test(io);
  // Firmware: Ping-Pong on MIDI→MIDI and CV→MIDI (not MIDI→CV — single jack).
  const pingPongActive =
    (midiToMidi || cvToMidi) && routing !== null && /ping/i.test(routing);

  if (cvToMidi) {
    inChannel = null;
    inUsb = null;
    inDin = null;
  }
  if (!pingPongActive && outChannels.length > 1) {
    outChannels = outChannels.slice(0, 1);
    outChannelNames = outChannelNames.slice(0, 1);
    outChannel = outChannels[0] ?? outChannel;
  }

  return {
    usbEnabled,
    out1,
    out2,
    channel: outChannel,
    outChannels,
    outChannelNames,
    inChannel,
    inUsb,
    inDin,
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

  drainConfigQueue(config.rx);
  const appsResponse = await sendAndReceive(config, { tag: "GetAllApps" });
  if (appsResponse.tag !== "BatchMsgStart") {
    throw new Error(`GetAllApps failed: ${appsResponse.tag}`);
  }
  const appMsgs = await receiveBatchMessages(config, appsResponse.value);
  const apps = new Map<number, AppMeta>();
  for (const item of appMsgs) {
    if (item.tag !== "AppConfig") continue;
    const [appIdRaw, channels, meta] = item.value;
    const appId = asU8(appIdRaw);
    apps.set(appId, {
      appId,
      channels: Number(channels),
      name: meta[1],
      description: meta[2],
      color: colorTag(meta[3]),
      icon: enumTag(meta[4]),
      params: meta[5],
    });
  }

  const layoutResponse = await sendAndReceive(config, { tag: "GetLayout" });
  if (layoutResponse.tag !== "Layout") {
    throw new Error(`GetLayout failed: ${layoutResponse.tag}`);
  }
  const layoutSlots = layoutResponse.value[0];

  // Prefer per-slot GetAppParams (layout_id verified) — GetAllAppParams can
  // time out mid-batch and leave a partial/confused picture after reconnect.
  const paramsByLayout = new Map<number, Value[]>();
  let lastUsed = -1;
  for (let startChannel = 0; startChannel < 16; startChannel++) {
    if (startChannel <= lastUsed) continue;
    const slot = layoutSlots[startChannel];
    if (!slot) {
      lastUsed = startChannel;
      continue;
    }
    const width = Math.max(1, Number(slot[1]) || 1);
    const layoutId = asU8(slot[2]);
    lastUsed = startChannel + width - 1;
    const values = await getAppParams(config, layoutId);
    if (values) paramsByLayout.set(layoutId, values);
  }

  const tracks = buildTracksFromLayout(layoutSlots, apps, paramsByLayout);
  const slotCount = countLayoutSlots(layoutSlots);
  if (slotCount > 0 && tracks.length === 0) {
    throw new Error(
      `Layout has ${slotCount} app(s) but none could be loaded (missing AppConfig / params). Close the Editor/Configurator and reconnect.`,
    );
  }
  if (slotCount > tracks.length) {
    console.warn(
      `Layout slots ${slotCount} → tracks ${tracks.length} (some AppConfig ids missing or params failed)`,
    );
  }

  return {
    version: config.version,
    apps,
    tracks,
    device,
  };
}

/** Re-read layout + live params (MIDI channels, Color, …) without reconnecting. */
export async function reloadTracks(snapshot: Snapshot): Promise<AppTrack[]> {
  const { config } = snapshot.device;
  drainConfigQueue(config.rx);
  const layoutResponse = await sendAndReceive(config, { tag: "GetLayout" });
  if (layoutResponse.tag !== "Layout") {
    throw new Error(`GetLayout failed: ${layoutResponse.tag}`);
  }
  const layoutSlots = layoutResponse.value[0];
  const paramsByLayout = new Map<number, Value[]>();
  let lastUsed = -1;
  for (let startChannel = 0; startChannel < 16; startChannel++) {
    if (startChannel <= lastUsed) continue;
    const slot = layoutSlots[startChannel];
    if (!slot) {
      lastUsed = startChannel;
      continue;
    }
    const width = Math.max(1, Number(slot[1]) || 1);
    const layoutId = asU8(slot[2]);
    lastUsed = startChannel + width - 1;
    const values = await getAppParams(config, layoutId);
    if (values) paramsByLayout.set(layoutId, values);
  }
  return buildTracksFromLayout(layoutSlots, snapshot.apps, paramsByLayout);
}

/** Soft-refresh: GetAppParams for existing slots only (no layout walk). */
export async function refreshAppParamsOnly(snapshot: Snapshot): Promise<AppTrack[]> {
  const { config } = snapshot.device;
  const { apps } = snapshot;
  const out: AppTrack[] = [];
  for (const track of snapshot.tracks) {
    if (config.rx.waiter) {
      // Don't stomp an in-flight request/response pair.
      out.push(track);
      continue;
    }
    const values = await getAppParams(config, track.layoutId);
    out.push(values ? applyAppStateToTrack(track, values, apps) : track);
  }
  return out;
}

export async function refreshTrackParams(snapshot: Snapshot): Promise<AppTrack[]> {
  // Prefer full layout reload — params-only refresh mis-pairs after slot swaps.
  return reloadTracks(snapshot);
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

    const currentValues = await getAppParams(config, track.layoutId);
    if (!currentValues) continue;
    const values = [...currentValues];
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
      value: { layout_id: asU8(track.layoutId), values: padParams(values) },
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

export function clampBpm(bpm: number): number {
  return Math.max(20, Math.min(300, Math.round(Number(bpm) || 120)));
}

export async function readClockConfig(
  snapshot: Snapshot,
): Promise<{ src: ClockSrcTag; bpm: number } | null> {
  const response = await sendAndReceive(snapshot.device.config, { tag: "GetGlobalConfig" });
  if (response.tag !== "GlobalConfig") return null;
  const clock = response.value.clock;
  return {
    src: clock.clock_src.tag,
    bpm: clampBpm(clock.internal_bpm),
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
  const bpm = clampBpm(gc.clock.internal_bpm);
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

/** Persist host tempo into device GlobalConfig.internal_bpm (FRAM). */
export async function writeDeviceBpm(
  snapshot: Snapshot,
  bpm: number,
): Promise<number | null> {
  const clamped = clampBpm(bpm);
  const response = await sendAndReceive(snapshot.device.config, { tag: "GetGlobalConfig" });
  if (response.tag !== "GlobalConfig") return null;
  const gc = response.value;
  if (clampBpm(gc.clock.internal_bpm) === clamped) return clamped;
  await sendAndReceive(snapshot.device.config, {
    tag: "SetGlobalConfig",
    value: {
      ...gc,
      clock: {
        ...gc.clock,
        internal_bpm: clamped,
      },
    },
  });
  return clamped;
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
 * Indices of *output* MidiChannel params (skip MidiIn channel when present).
 * Matches extractMidi: first MidiChannel after MidiIn+before MidiOut = input.
 */
function outMidiChannelIndices(params: Param[]): number[] {
  let sawMidiIn = false;
  let sawMidiOut = false;
  let inTaken = false;
  const idxs: number[] = [];
  params.forEach((p, i) => {
    if (p.tag === "MidiIn") sawMidiIn = true;
    if (p.tag === "MidiOut") sawMidiOut = true;
    if (p.tag !== "MidiChannel") return;
    if (sawMidiIn && !inTaken && !sawMidiOut) {
      inTaken = true;
      return;
    }
    idxs.push(i);
  });
  return idxs;
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
    for (const ch of track.midi.outChannels) {
      if (ch >= 1 && ch <= 16) used.add(ch);
    }
    if (track.midi.inChannel && track.midi.inChannel >= 1) {
      used.add(track.midi.inChannel);
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

    const values = await getAppParams(config, track.layoutId);
    if (!values) {
      console.warn(`Unique MIDI: no AppState for ${track.app.name}`);
      continue;
    }

    const outIdxs = outMidiChannelIndices(track.app.params);
    if (outIdxs.length === 0) continue;

    // Always give every colliding app fresh out channel(s). If alloc() returns the
    // channel they already use (everyone on CH1), keep allocating so Unique MIDI
    // actually moves them apart instead of no-op'ing the first peer.
    const sparse = padParams([]);
    let wrote = false;
    for (const chIdx of outIdxs) {
      const currentVal = values[chIdx];
      const current =
        currentVal?.tag === "MidiChannel"
          ? Number(Array.isArray(currentVal.value) ? currentVal.value[0] : currentVal.value)
          : null;
      let ch = alloc();
      while (ch !== null && current !== null && ch === current) {
        ch = alloc();
      }
      if (ch === null) break;
      sparse[chIdx] = { tag: "MidiChannel", value: [ch] };
      wrote = true;
    }
    if (!wrote) continue;

    const setResponse = await sendAndReceive(config, {
      tag: "SetAppParams",
      value: { layout_id: asU8(track.layoutId), values: sparse },
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
