import type {
  ConfigMsgOut,
  FpAppSection,
  FpAppStatus,
  FixedLengthArray,
} from "@atov/fp-config";

import {
  sendAndReceive,
  sendAndReceiveBatch,
  type FpMidiDevice,
} from "./midi-protocol";
import type { AllColors, App, AllApps } from "./types";

const MAGIC = [0x46, 0x50, 0x41, 0x50, 0x50, 0x00, 0x0d, 0x0a];
const NATIVE_MAGIC = [0x46, 0x50, 0x4e, 0x30];
const CONTAINER_VERSION = 0;
const RUNTIME_ABI_VERSION = 1;
const HEADER_SIZE = 24;
const DESCRIPTOR_SIZE = 12;
const CHUNK_SIZE = 256;
const CHUNK_BLOCK_SIZE = 32;
const MANUAL_CACHE_KEY = `fpapp-manuals:${import.meta.env.BASE_URL}`;
const STRUCTURED_MANUAL_FORMAT = "faderpunk-manual-v1";
const MANUAL_COLORS = new Set<AllColors>([
  "White",
  "Yellow",
  "Orange",
  "Red",
  "Lime",
  "Green",
  "Cyan",
  "SkyBlue",
  "Blue",
  "Violet",
  "Pink",
  "PaleGreen",
  "Sand",
  "Rose",
  "Salmon",
  "LightBlue",
  "Black",
  "Custom",
]);

export const FPAPP_MANUALS_UPDATED_EVENT = "fpapp-manuals-updated";

export type FpAppSupport = Extract<
  ConfigMsgOut,
  { tag: "FpAppSupport" }
>["value"];
export type InstalledFpApp = Extract<
  ConfigMsgOut,
  { tag: "FpAppSlot" }
>["value"];

export interface FpAppSlot {
  slot: number;
  app?: InstalledFpApp;
}

export interface ParsedFpApp {
  bytes: Uint8Array;
  appId: number;
  version: string;
  name: string;
  description: string;
  author: string;
  channels: number;
  firmwareAbi: string;
  manual?: string;
  setup?: string;
  settings?: string;
  signed: boolean;
  nativeImageBytes: number;
  appConfig?: Pick<App, "color" | "icon" | "params">;
}

export interface CachedFpAppManual {
  appId: number;
  name: string;
  description: string;
  manual?: string;
  setup?: string;
}

export interface FpAppManualChannel {
  jackTitle: string;
  jackDescription: string;
  faderTitle: string;
  faderDescription: string;
  faderPlusFnTitle?: string;
  faderPlusFnDescription?: string;
  faderPlusShiftTitle?: string;
  faderPlusShiftDescription?: string;
  fnTitle?: string;
  fnDescription?: string;
  fnPlusShiftTitle?: string;
  fnPlusShiftDescription?: string;
  ledTop: string;
  ledTopPlusShift?: string;
  ledTopPlusFn?: string;
  ledBottom: string;
  ledBottomPlusShift?: string;
  ledBottomPlusFn?: string;
}

export interface FpAppManualData {
  appId: number;
  title: string;
  description: string;
  icon: string;
  color: AllColors;
  params?: string[];
  storage?: string[];
  text: string;
  channels: FpAppManualChannel[];
}

interface Section {
  kind: number;
  flags: number;
  offset: number;
  length: number;
  bytes: Uint8Array;
}

export async function parseFpApp(file: File): Promise<ParsedFpApp> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (bytes.length < HEADER_SIZE || !matches(bytes.subarray(0, 8), MAGIC)) {
    throw new Error("This is not a Faderpunk .fpapp file.");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (
    view.getUint16(8, true) !== CONTAINER_VERSION ||
    view.getUint16(10, true) !== RUNTIME_ABI_VERSION
  ) {
    throw new Error("This .fpapp format is newer than this configurator.");
  }
  if (view.getUint32(12, true) !== bytes.length) {
    throw new Error("The .fpapp file is truncated or has trailing data.");
  }
  const sectionCount = view.getUint16(16, true);
  const tableEnd = view.getUint16(18, true);
  if (
    sectionCount > 16 ||
    tableEnd !== HEADER_SIZE + sectionCount * DESCRIPTOR_SIZE ||
    tableEnd > bytes.length
  ) {
    throw new Error("The .fpapp section table is invalid.");
  }
  if (crc32(bytes.subarray(tableEnd)) !== view.getUint32(20, true)) {
    throw new Error("The .fpapp checksum does not match.");
  }

  const sections = new Map<number, Section>();
  const ranges: Array<[number, number]> = [];
  for (let index = 0; index < sectionCount; index++) {
    const descriptor = HEADER_SIZE + index * DESCRIPTOR_SIZE;
    const kind = view.getUint16(descriptor, true);
    const flags = view.getUint16(descriptor + 2, true);
    const offset = view.getUint32(descriptor + 4, true);
    const length = view.getUint32(descriptor + 8, true);
    const end = offset + length;
    if (
      offset < tableEnd ||
      offset % 4 !== 0 ||
      end > bytes.length ||
      end < offset ||
      ranges.some(([start, previousEnd]) =>
        overlaps(offset, end, start, previousEnd),
      )
    ) {
      throw new Error("The .fpapp contains an invalid or overlapping section.");
    }
    if (sections.has(kind)) {
      throw new Error("The .fpapp contains a duplicate section.");
    }
    ranges.push([offset, end]);
    sections.set(kind, {
      kind,
      flags,
      offset,
      length,
      bytes: bytes.subarray(offset, end),
    });
  }
  for (const section of sections.values()) {
    if (![1, 2, 3, 4, 5, 6].includes(section.kind) && section.flags & 1) {
      throw new Error("The .fpapp needs an unsupported required feature.");
    }
  }
  const manifestSection = sections.get(1);
  const programSection = sections.get(2);
  if (!manifestSection || !programSection) {
    throw new Error("The .fpapp is missing its manifest or native program.");
  }
  const manifest = parseManifest(manifestSection.bytes);
  if (manifest.programKind !== 1) {
    throw new Error("Only native Thumb ROPI FPApps are supported.");
  }
  if (
    programSection.bytes.length < 28 ||
    !matches(programSection.bytes.subarray(0, 4), NATIVE_MAGIC) ||
    new DataView(
      programSection.bytes.buffer,
      programSection.bytes.byteOffset,
      programSection.bytes.byteLength,
    ).getUint32(24, true) !==
      programSection.bytes.length - 28
  ) {
    throw new Error("The .fpapp native image header is invalid.");
  }
  const decodeText = (kind: number) => {
    const section = sections.get(kind);
    return section
      ? new TextDecoder("utf-8", { fatal: true }).decode(section.bytes)
      : undefined;
  };
  const manual = decodeText(3);
  if (manual) parseFpAppManual(manual, manifest.appId);
  const settings = decodeText(5);
  return {
    bytes,
    appId: manifest.appId,
    version: manifest.version.join("."),
    name: manifest.name,
    description: manifest.description,
    author: manifest.author,
    channels: manifest.channels,
    firmwareAbi: toHex(manifest.firmwareAbi),
    manual,
    setup: decodeText(4),
    settings,
    signed: sections.has(6),
    nativeImageBytes: programSection.bytes.length - 28,
    appConfig: settings ? parseAppConfig(settings) : undefined,
  };
}

export function parseFpAppManual(
  contents: string,
  expectedAppId: number,
): FpAppManualData | undefined {
  let document: unknown;
  try {
    document = JSON.parse(contents);
  } catch {
    return undefined;
  }
  if (!document || typeof document !== "object") return undefined;
  const envelope = document as { format?: unknown; app?: unknown };
  if (envelope.format !== STRUCTURED_MANUAL_FORMAT) return undefined;
  if (!isFpAppManualData(envelope.app, expectedAppId)) {
    throw new Error("The FPApp structured manual is invalid.");
  }
  return envelope.app;
}

function isFpAppManualData(
  value: unknown,
  expectedAppId: number,
): value is FpAppManualData {
  if (!value || typeof value !== "object") return false;
  const manual = value as Partial<FpAppManualData>;
  return (
    manual.appId === expectedAppId &&
    typeof manual.title === "string" &&
    manual.title.length > 0 &&
    typeof manual.description === "string" &&
    manual.description.length > 0 &&
    typeof manual.icon === "string" &&
    manual.icon.length > 0 &&
    typeof manual.color === "string" &&
    MANUAL_COLORS.has(manual.color as AllColors) &&
    typeof manual.text === "string" &&
    manual.text.length > 0 &&
    Array.isArray(manual.channels) &&
    manual.channels.length > 0 &&
    manual.channels.every(isFpAppManualChannel) &&
    isOptionalStringArray(manual.params) &&
    isOptionalStringArray(manual.storage)
  );
}

function isFpAppManualChannel(value: unknown): value is FpAppManualChannel {
  if (!value || typeof value !== "object") return false;
  const channel = value as Partial<FpAppManualChannel>;
  const required = [
    channel.jackTitle,
    channel.jackDescription,
    channel.faderTitle,
    channel.faderDescription,
    channel.ledTop,
    channel.ledBottom,
  ];
  const optional = [
    channel.faderPlusFnTitle,
    channel.faderPlusFnDescription,
    channel.faderPlusShiftTitle,
    channel.faderPlusShiftDescription,
    channel.fnTitle,
    channel.fnDescription,
    channel.fnPlusShiftTitle,
    channel.fnPlusShiftDescription,
    channel.ledTopPlusShift,
    channel.ledTopPlusFn,
    channel.ledBottomPlusShift,
    channel.ledBottomPlusFn,
  ];
  return (
    required.every((field) => typeof field === "string") &&
    optional.every((field) => field === undefined || typeof field === "string")
  );
}

function isOptionalStringArray(value: unknown) {
  return (
    value === undefined ||
    (Array.isArray(value) && value.every((item) => typeof item === "string"))
  );
}

export async function mergeInstalledFpAppConfigs(
  device: FpMidiDevice,
  apps: AllApps,
): Promise<AllApps> {
  const merged = new Map(apps);
  const slots = await getFpAppSlots(device);
  const manuals: CachedFpAppManual[] = [];
  for (const slot of slots) {
    if (!slot.app) continue;
    if (slot.app.has_settings) {
      try {
        const settings = await readFpAppSection(device, slot.slot, {
          tag: "Settings",
        });
        const config = parseAppConfig(settings);
        const app = merged.get(slot.app.app_id);
        if (config && app) {
          merged.set(slot.app.app_id, {
            ...app,
            ...config,
            paramCount: BigInt(config.params.length),
          });
        }
      } catch {
        // A package without valid Configurator metadata remains usable with no
        // parameters. Its installer record is still shown below the catalog.
      }
    }

    const manual = slot.app.has_manual
      ? await readOptionalFpAppSection(device, slot.slot, { tag: "Manual" })
      : undefined;
    const setup = slot.app.has_setup
      ? await readOptionalFpAppSection(device, slot.slot, { tag: "Setup" })
      : undefined;
    if (manual || setup) {
      manuals.push({
        appId: slot.app.app_id,
        name: slot.app.name,
        description: slot.app.description,
        manual,
        setup,
      });
    }
  }
  saveCachedFpAppManuals(manuals);
  return merged;
}

async function readOptionalFpAppSection(
  device: FpMidiDevice,
  slot: number,
  section: FpAppSection,
) {
  try {
    return await readFpAppSection(device, slot, section);
  } catch {
    return undefined;
  }
}

export function getCachedFpAppManuals(): CachedFpAppManual[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const parsed = JSON.parse(localStorage.getItem(MANUAL_CACHE_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isCachedFpAppManual);
  } catch {
    return [];
  }
}

export function cacheFpAppManual(app: ParsedFpApp) {
  const manuals = getCachedFpAppManuals().filter(
    (manual) => manual.appId !== app.appId,
  );
  if (app.manual || app.setup) {
    manuals.push({
      appId: app.appId,
      name: app.name,
      description: app.description,
      manual: app.manual,
      setup: app.setup,
    });
  }
  saveCachedFpAppManuals(manuals);
}

export function removeCachedFpAppManual(appId: number) {
  saveCachedFpAppManuals(
    getCachedFpAppManuals().filter((manual) => manual.appId !== appId),
  );
}

function saveCachedFpAppManuals(manuals: CachedFpAppManual[]) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(MANUAL_CACHE_KEY, JSON.stringify(manuals));
    window.dispatchEvent(new Event(FPAPP_MANUALS_UPDATED_EVENT));
  } catch {
    // The catalog remains usable when browser storage is unavailable.
  }
}

function isCachedFpAppManual(value: unknown): value is CachedFpAppManual {
  if (!value || typeof value !== "object") return false;
  const manual = value as Partial<CachedFpAppManual>;
  return (
    typeof manual.appId === "number" &&
    typeof manual.name === "string" &&
    typeof manual.description === "string" &&
    (manual.manual === undefined || typeof manual.manual === "string") &&
    (manual.setup === undefined || typeof manual.setup === "string")
  );
}

function parseAppConfig(
  contents: string,
): Pick<App, "color" | "icon" | "params"> | undefined {
  try {
    const parsed = JSON.parse(contents) as {
      format?: string;
      app?: Pick<App, "color" | "icon" | "params">;
    };
    if (
      parsed.format !== "faderpunk-app-config-v1" ||
      !parsed.app ||
      typeof parsed.app.color !== "string" ||
      typeof parsed.app.icon !== "string" ||
      !Array.isArray(parsed.app.params)
    ) {
      return undefined;
    }
    return parsed.app;
  } catch {
    return undefined;
  }
}

export async function getFpAppSupport(
  device: FpMidiDevice,
): Promise<FpAppSupport> {
  const response = await sendAndReceive(device, { tag: "GetFpAppSupport" });
  if (response.tag !== "FpAppSupport") {
    throw new Error(`Expected FpAppSupport, received ${response.tag}.`);
  }
  return response.value;
}

export async function getFpAppSlots(
  device: FpMidiDevice,
): Promise<FpAppSlot[]> {
  const { start, messages } = await sendAndReceiveBatch(device, {
    tag: "GetFpAppSlots",
  });
  if (start.tag !== "BatchMsgStart") {
    throw new Error(`Expected FPApp slot batch, received ${start.tag}.`);
  }
  return messages.map((message) => {
    if (message.tag === "FpAppSlot") {
      return { slot: message.value.slot, app: message.value };
    }
    if (message.tag === "FpAppSlotEmpty") {
      return { slot: message.value.slot };
    }
    throw new Error(`Unexpected FPApp slot response ${message.tag}.`);
  });
}

export async function installFpApp(
  device: FpMidiDevice,
  slot: number,
  app: ParsedFpApp,
  support: FpAppSupport,
  onProgress: (progress: number) => void,
) {
  if (app.firmwareAbi !== toHex(support.firmware_abi)) {
    throw new Error("This FPApp was compiled for a different firmware build.");
  }
  if (app.bytes.length > support.max_package_len) {
    throw new Error(
      `This FPApp is ${app.bytes.length} bytes; the device limit is ${support.max_package_len}.`,
    );
  }
  await expectOk(
    await sendAndReceive(
      device,
      {
        tag: "BeginFpAppInstall",
        value: { slot, total_len: app.bytes.length },
      },
      15_000,
    ),
  );
  try {
    const chunkSize = Math.min(support.chunk_size, CHUNK_SIZE);
    for (let offset = 0; offset < app.bytes.length; offset += chunkSize) {
      const chunk = app.bytes.subarray(
        offset,
        Math.min(offset + chunkSize, app.bytes.length),
      );
      await expectOk(
        await sendAndReceive(device, {
          tag: "WriteFpAppChunk",
          value: {
            offset,
            len: chunk.length,
            data: chunkBlocks(chunk),
          },
        }),
      );
      onProgress((offset + chunk.length) / app.bytes.length);
    }
    await expectOk(
      await sendAndReceive(device, { tag: "CommitFpAppInstall" }, 30_000),
    );
  } catch (error) {
    try {
      await sendAndReceive(device, { tag: "AbortFpAppInstall" });
    } catch {
      // Preserve the original upload error. The slot is already invalid and a
      // fresh BeginFpAppInstall after reconnect can recover it.
    }
    throw error;
  }
}

export async function removeFpApp(device: FpMidiDevice, slot: number) {
  await expectOk(
    await sendAndReceive(device, { tag: "RemoveFpApp", value: { slot } }),
  );
}

export async function readFpAppSection(
  device: FpMidiDevice,
  slot: number,
  section: FpAppSection,
): Promise<string> {
  const bytes: number[] = [];
  let offset = 0;
  let total = 1;
  while (offset < total) {
    const response = await sendAndReceive(device, {
      tag: "ReadFpAppSection",
      value: { slot, section, offset },
    });
    if (response.tag !== "FpAppSectionChunk") {
      await expectOk(response);
      throw new Error(`Expected FPApp section data, received ${response.tag}.`);
    }
    total = response.value.total_len;
    const flat = response.value.data.flat();
    bytes.push(...flat.slice(0, response.value.len));
    offset += response.value.len;
    if (response.value.len === 0 && offset < total) {
      throw new Error("The device returned an empty FPApp section chunk.");
    }
  }
  return new TextDecoder().decode(new Uint8Array(bytes));
}

export function abiHex(abi: ArrayLike<number>) {
  return toHex(abi);
}

function expectOk(response: ConfigMsgOut) {
  if (response.tag !== "FpAppResult") {
    throw new Error(`Expected FPApp result, received ${response.tag}.`);
  }
  if (response.value.tag !== "Ok") {
    throw new Error(statusMessage(response.value));
  }
}

function statusMessage(status: FpAppStatus) {
  const messages: Record<FpAppStatus["tag"], string> = {
    Ok: "The FPApp operation completed.",
    InvalidSlot: "That FPApp slot does not exist.",
    Busy: "Another FPApp upload is already in progress.",
    NoInstall: "There is no FPApp upload in progress.",
    EmptyPackage: "The selected FPApp is empty.",
    PackageTooLarge: "The selected FPApp is too large for one slot.",
    UnexpectedOffset: "An FPApp upload chunk arrived out of order.",
    ChunkTooLarge: "An FPApp upload chunk is too large.",
    Incomplete: "The FPApp upload did not finish.",
    ActiveApp:
      "The installed app is active in the channel layout. Remove it from the layout, then try again.",
    IncompatibleFirmware: "This FPApp was compiled for different firmware.",
    DuplicateAppId: "This FPApp is already installed in another slot.",
    InvalidPackage: "The device rejected the FPApp package.",
    FlashError: "The device could not write its FPApp flash region.",
  };
  return messages[status.tag];
}

function chunkBlocks(
  chunk: Uint8Array,
): FixedLengthArray<FixedLengthArray<number, 32>, 8> {
  const blocks = Array.from({ length: 8 }, (_, blockIndex) =>
    Array.from(
      { length: CHUNK_BLOCK_SIZE },
      (_, byteIndex) => chunk[blockIndex * CHUNK_BLOCK_SIZE + byteIndex] ?? 0,
    ),
  );
  return blocks as unknown as FixedLengthArray<FixedLengthArray<number, 32>, 8>;
}

function parseManifest(bytes: Uint8Array) {
  const decoder = new CborDecoder(bytes);
  const fieldCount = decoder.map();
  const result: Record<number, unknown> = {};
  for (let index = 0; index < fieldCount; index++) {
    const key = decoder.uint();
    switch (key) {
      case 0:
      case 2:
      case 6:
      case 7:
      case 8:
      case 10:
      case 11:
      case 12:
        result[key] = decoder.uint();
        break;
      case 1: {
        const count = decoder.array();
        if (count !== 3) throw new Error("The FPApp version is invalid.");
        result[key] = [decoder.uint(), decoder.uint(), decoder.uint()];
        break;
      }
      case 3:
      case 4:
      case 5:
        result[key] = decoder.text();
        break;
      case 9:
        result[key] = decoder.array();
        for (
          let parameter = 0;
          parameter < (result[key] as number);
          parameter++
        ) {
          decoder.skip();
        }
        break;
      case 13:
        result[key] = decoder.bytes();
        break;
      default:
        decoder.skip();
    }
  }
  if (
    typeof result[0] !== "number" ||
    !Array.isArray(result[1]) ||
    typeof result[2] !== "number" ||
    typeof result[3] !== "string" ||
    typeof result[4] !== "string" ||
    typeof result[5] !== "string" ||
    typeof result[6] !== "number" ||
    !(result[13] instanceof Uint8Array) ||
    result[13].length !== 32
  ) {
    throw new Error("The FPApp manifest is incomplete.");
  }
  return {
    appId: result[0],
    version: result[1] as number[],
    programKind: result[2],
    name: result[3],
    description: result[4],
    author: result[5],
    channels: result[6],
    firmwareAbi: result[13],
  };
}

class CborDecoder {
  private offset = 0;

  constructor(private readonly bytesValue: Uint8Array) {}

  uint() {
    const [major, value] = this.head();
    if (major !== 0) throw new Error("Expected a CBOR unsigned integer.");
    return value;
  }

  array() {
    const [major, value] = this.head();
    if (major !== 4) throw new Error("Expected a CBOR array.");
    return value;
  }

  map() {
    const [major, value] = this.head();
    if (major !== 5) throw new Error("Expected a CBOR map.");
    return value;
  }

  bytes() {
    const [major, length] = this.head();
    if (major !== 2) throw new Error("Expected CBOR bytes.");
    return this.take(length);
  }

  text() {
    const [major, length] = this.head();
    if (major !== 3) throw new Error("Expected CBOR text.");
    return new TextDecoder("utf-8", { fatal: true }).decode(this.take(length));
  }

  skip() {
    const [major, value] = this.head();
    if (major === 2 || major === 3) {
      this.take(value);
    } else if (major === 4) {
      for (let index = 0; index < value; index++) this.skip();
    } else if (major === 5) {
      for (let index = 0; index < value; index++) {
        this.skip();
        this.skip();
      }
    }
  }

  private head(): [number, number] {
    const initial = this.take(1)[0];
    const major = initial >> 5;
    const additional = initial & 0x1f;
    if (additional < 24) return [major, additional];
    if (additional === 24) return [major, this.take(1)[0]];
    if (additional === 25) {
      const bytes = this.take(2);
      return [major, (bytes[0] << 8) | bytes[1]];
    }
    if (additional === 26) {
      const bytes = this.take(4);
      return [
        major,
        (bytes[0] * 0x1000000 +
          (bytes[1] << 16) +
          (bytes[2] << 8) +
          bytes[3]) >>>
          0,
      ];
    }
    throw new Error("Unsupported CBOR length in FPApp manifest.");
  }

  private take(length: number) {
    const end = this.offset + length;
    if (end > this.bytesValue.length || end < this.offset) {
      throw new Error("The FPApp manifest is truncated.");
    }
    const value = this.bytesValue.subarray(this.offset, end);
    this.offset = end;
    return value;
  }
}

function crc32(bytes: Uint8Array) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return ~crc >>> 0;
}

function toHex(value: ArrayLike<number>) {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

function matches(value: Uint8Array, expected: number[]) {
  return expected.every((byte, index) => value[index] === byte);
}

function overlaps(startA: number, endA: number, startB: number, endB: number) {
  return startA < endB && startB < endA;
}
