import {
  type ConfigMsgIn,
  type ConfigMsgOut,
  deserialize,
  serialize,
} from "@atov/fp-config";

import { buildConfigFrame, parseConfigFrame, SYSEX_EOX, SYSEX_START } from "./sysex";

const RECEIVE_TIMEOUT_MS = 2000;
const PROBE_TIMEOUT_MS = 300;

interface Waiter {
  resolve: (msg: ConfigMsgOut) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface RxState {
  sysexBuffer: number[];
  collecting: boolean;
  queue: ConfigMsgOut[];
  waiter: Waiter | null;
}

export type PerfHandler = (data: Uint8Array, t: number) => void;
/** Unsolicited config pushes (AppState after device param edits, Layout, …). */
export type ConfigPushHandler = (msg: ConfigMsgOut) => void;

export interface ConfigPort {
  input: MIDIInput;
  output: MIDIOutput;
  version: string;
  rx: RxState;
}

function attachConfigInput(
  input: MIDIInput,
  onPerf?: PerfHandler,
  onPush?: ConfigPushHandler,
): RxState {
  const rx: RxState = {
    sysexBuffer: [],
    collecting: false,
    queue: [],
    waiter: null,
  };

  input.onmidimessage = (event: MIDIMessageEvent) => {
    if (!event.data || event.data.length === 0) return;
    const data = event.data;
    const t = performance.now();
    const first = data[0];

    // Complete non-SysEx messages (notes/CC/clock) — forward as performance MIDI.
    // Web MIDI usually delivers one message per event.
    if (first !== SYSEX_START) {
      // While collecting a fragmented SysEx, ignore interleaved noise
      if (!rx.collecting) onPerf?.(new Uint8Array(data), t);
      // Fall through only if somehow mixed (rare); still try sysex scan below
      if (first < 0xf0 || first === 0xf8 || first === 0xfa || first === 0xfb || first === 0xfc || first === 0xff) {
        return;
      }
    }

    for (const byte of data) {
      if (byte === SYSEX_START) {
        rx.sysexBuffer = [byte];
        rx.collecting = true;
        continue;
      }
      if (!rx.collecting) continue;
      rx.sysexBuffer.push(byte);
      if (byte === SYSEX_EOX) {
        rx.collecting = false;
        const payload = parseConfigFrame(new Uint8Array(rx.sysexBuffer));
        rx.sysexBuffer = [];
        if (!payload) continue;
        let msg: ConfigMsgOut;
        try {
          msg = deserialize("ConfigMsgOut", payload).value;
        } catch (err) {
          console.error("Failed to deserialize config message:", err);
          continue;
        }
        if (rx.waiter) {
          const { resolve, timer } = rx.waiter;
          clearTimeout(timer);
          rx.waiter = null;
          resolve(msg);
        } else if (
          onPush &&
          (msg.tag === "AppState" || msg.tag === "Layout")
        ) {
          onPush(msg);
        } else {
          rx.queue.push(msg);
        }
      }
    }
  };

  return rx;
}

function receiveFromRx(rx: RxState, timeoutMs: number): Promise<ConfigMsgOut> {
  const queued = rx.queue.shift();
  if (queued) return Promise.resolve(queued);
  if (rx.waiter) {
    return Promise.reject(new Error("Concurrent receive on the same MIDI device"));
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      rx.waiter = null;
      reject(new Error("Timed out waiting for device response"));
    }, timeoutMs);
    rx.waiter = { resolve, reject, timer };
  });
}

function sendFrame(output: MIDIOutput, msg: ConfigMsgIn) {
  output.send(Array.from(buildConfigFrame(serialize("ConfigMsgIn", msg))));
}

async function probePair(
  input: MIDIInput,
  output: MIDIOutput,
): Promise<string | null> {
  const rx = attachConfigInput(input);
  try {
    await input.open();
    await output.open();
    sendFrame(output, { tag: "GetVersion" });
    const msg = await receiveFromRx(rx, PROBE_TIMEOUT_MS);
    if (msg.tag === "Version") {
      const { major, minor, patch } = msg.value;
      return `${major}.${minor}.${patch}`;
    }
    return null;
  } catch {
    return null;
  } finally {
    input.onmidimessage = null;
  }
}

function portCandidates<T extends MIDIPort>(ports: Iterable<T>): T[] {
  const candidates = Array.from(ports).filter((port) =>
    /faderpunk/i.test(`${port.manufacturer ?? ""} ${port.name ?? ""}`),
  );
  return candidates.sort((a, b) => {
    const rank = (port: T) => (/config|2/i.test(port.name ?? "") ? 0 : 1);
    return rank(a) - rank(b);
  });
}

export interface DeviceBundle {
  access: MIDIAccess;
  config: ConfigPort;
  /** All Faderpunk inputs that may carry performance MIDI (including config port). */
  performanceInputs: MIDIInput[];
  /** Non-config outputs — used for MIDI panic (All Notes/Sound Off). */
  performanceOutputs: MIDIOutput[];
  /** True when a distinct non-config MIDI out exists (USB cable 0). */
  hasDedicatedPerfOut: boolean;
  portSummary: string;
}

export async function connectDevice(): Promise<DeviceBundle> {
  if (!navigator.requestMIDIAccess) {
    throw new Error("Web MIDI is not supported in this browser");
  }
  const access = await navigator.requestMIDIAccess({ sysex: true });
  const inputs = portCandidates(access.inputs.values());
  const outputs = portCandidates(access.outputs.values());

  let config: ConfigPort | null = null;
  for (const output of outputs) {
    for (const input of inputs) {
      const version = await probePair(input, output);
      if (version === null) continue;
      // Handler attached later once we have onPerf from the app layer
      const rx = attachConfigInput(input);
      config = { input, output, version, rx };
      break;
    }
    if (config) break;
  }

  if (!config) {
    throw new Error("No Faderpunk config MIDI port found");
  }

  const performanceInputs: MIDIInput[] = [];
  for (const input of inputs) {
    await input.open();
    performanceInputs.push(input);
  }

  const performanceOutputs: MIDIOutput[] = [];
  for (const output of outputs) {
    if (output.id === config.output.id) continue;
    await output.open();
    performanceOutputs.push(output);
  }
  const hasDedicatedPerfOut = performanceOutputs.length > 0;
  // If the host only exposes one out, still allow panic on it (channel msgs are fine)
  if (performanceOutputs.length === 0) {
    performanceOutputs.push(config.output);
  }

  const inNames = inputs.map((i) => i.name ?? i.id).join(", ");
  const outNames = outputs.map((o) => o.name ?? o.id).join(", ");
  const portSummary = `in[${inNames}] · out[${outNames}]${hasDedicatedPerfOut ? "" : " · ⚠ single out (=config cable)"}`;

  return {
    access,
    config,
    performanceInputs,
    performanceOutputs,
    hasDedicatedPerfOut,
    portSummary,
  };
}

/** Attach performance + config handlers. Call after connect once you have onPerf. */
export function bindMidiHandlers(
  device: DeviceBundle,
  onPerf: PerfHandler,
  onPush?: ConfigPushHandler,
): void {
  // Config port: SysEx + performance
  device.config.rx = attachConfigInput(device.config.input, onPerf, onPush);

  for (const input of device.performanceInputs) {
    if (input.id === device.config.input.id) continue;
    input.onmidimessage = (event: MIDIMessageEvent) => {
      if (!event.data) return;
      onPerf(new Uint8Array(event.data), performance.now());
    };
  }
}

export function unbindMidiHandlers(device: DeviceBundle): void {
  device.config.input.onmidimessage = null;
  for (const input of device.performanceInputs) {
    input.onmidimessage = null;
  }
}

/** Drop handlers and close every port we opened — full release of the device. */
export async function releaseDevice(device: DeviceBundle): Promise<void> {
  unbindMidiHandlers(device);
  const ports: MIDIPort[] = [
    device.config.input,
    device.config.output,
    ...device.performanceInputs,
    ...device.performanceOutputs,
  ];
  const seen = new Set<string>();
  await Promise.all(
    ports.map(async (port) => {
      if (seen.has(port.id)) return;
      seen.add(port.id);
      try {
        if (port.connection === "open" || port.state === "connected") {
          await port.close();
        }
      } catch {
        /* already closed / browser quirk */
      }
    }),
  );
}

/** Drop unmatched config replies so the next request/response pair stays aligned. */
export function drainConfigQueue(rx: RxState): void {
  rx.queue.length = 0;
}

export async function sendAndReceive(
  config: ConfigPort,
  msg: ConfigMsgIn,
): Promise<ConfigMsgOut> {
  sendFrame(config.output, msg);
  return receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS);
}

export async function receiveBatchMessages(
  config: ConfigPort,
  count: bigint,
): Promise<ConfigMsgOut[]> {
  const results: ConfigMsgOut[] = [];
  for (let i = 0n; i < count; i++) {
    results.push(await receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS));
  }
  const endMessage = await receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS);
  if (endMessage.tag !== "BatchMsgEnd") {
    throw new Error("Expected BatchMsgEnd but received: " + endMessage.tag);
  }
  return results;
}
