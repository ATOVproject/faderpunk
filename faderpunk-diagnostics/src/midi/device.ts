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

interface RxState {
  sysexBuffer: number[];
  collecting: boolean;
  queue: ConfigMsgOut[];
  waiter: Waiter | null;
}

export interface ConfigPort {
  input: MIDIInput;
  output: MIDIOutput;
  version: string;
  rx: RxState;
}

function attachConfigInput(input: MIDIInput): RxState {
  const rx: RxState = {
    sysexBuffer: [],
    collecting: false,
    queue: [],
    waiter: null,
  };

  input.onmidimessage = (event: MIDIMessageEvent) => {
    if (!event.data) return;
    for (const byte of event.data) {
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
  performanceInput: MIDIInput | null;
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
      const rx = attachConfigInput(input);
      config = { input, output, version, rx };
      break;
    }
    if (config) break;
  }

  if (!config) {
    throw new Error("No Faderpunk config MIDI port found");
  }

  const performanceInput =
    inputs.find((input) => input.id !== config!.input.id) ?? null;
  if (performanceInput) await performanceInput.open();

  return { access, config, performanceInput };
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
