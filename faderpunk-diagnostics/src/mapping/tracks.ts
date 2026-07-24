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
  usbEnabled: boolean;
  channel: number; // 1–16
  cc: number | null; // 0–127 when CC app
  noteMode: boolean; // true = primarily notes
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

function midiNrpn(value: Value | undefined): boolean {
  return value?.tag === "MidiNrpn" ? value.value : false;
}

function inferNoteMode(params: Param[], values: Value[]): boolean {
  const modeIdx = params.findIndex((p) => p.tag === "MidiMode");
  if (modeIdx >= 0) {
    const explicit = midiModeNote(values[modeIdx]);
    if (explicit !== null) return explicit;
  }
  // Apps with MidiCc tend to be continuous; MidiNote / sequencers emit notes.
  const hasCc = params.some((p) => p.tag === "MidiCc");
  const hasNote = params.some((p) => p.tag === "MidiNote");
  if (hasNote && !hasCc) return true;
  if (hasCc && !hasNote) return false;
  // Heuristic by name tokens later — default continuous if CC present.
  return !hasCc;
}

function extractMidi(params: Param[], values: Value[], appName: string): TrackMidi {
  let channel = 1;
  let cc: number | null = null;
  let usbEnabled = false;
  let nrpn = false;

  params.forEach((param, i) => {
    const value = values[i];
    switch (param.tag) {
      case "MidiChannel":
        channel = midiChannelFromValue(value);
        break;
      case "MidiCc":
        cc = midiCcFromValue(value);
        break;
      case "MidiOut":
        usbEnabled = midiOutUsb(value);
        break;
      case "MidiNrpn":
        nrpn = midiNrpn(value);
        break;
      default:
        break;
    }
  });

  const noteMode =
    /seq|euclid|turing|grids|tb3|bernoulli|trigger|note|clk/i.test(appName) ||
    inferNoteMode(params, values);

  return { usbEnabled, channel, cc, noteMode, nrpn };
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
