import {
  type ConfigMsgIn,
  type ConfigMsgOut,
  deserialize,
  serialize,
} from "@atov/fp-config";

import {
  buildConfigFrame,
  parseConfigFrame,
  SYSEX_EOX,
  SYSEX_START,
} from "./sysex";

// Timeout for a regular protocol response. Also what lets the connection
// health check detect an unplugged device (Web MIDI has no blocking read).
const RECEIVE_TIMEOUT_MS = 2000;
// Timeout for the GetVersion probe during port discovery
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

export interface FpMidiDevice {
  access: MIDIAccess;
  input: MIDIInput;
  output: MIDIOutput;
  version: string;
  rx: RxState;
}

function attachInput(input: MIDIInput): RxState {
  const rx: RxState = {
    sysexBuffer: [],
    collecting: false,
    queue: [],
    waiter: null,
  };

  input.onmidimessage = (event: MIDIMessageEvent) => {
    if (!event.data) return;
    // Chromium delivers complete SysEx messages in a single event, but
    // accumulate defensively in case an implementation fragments them.
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
        if (!payload) continue; // foreign or corrupt SysEx
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

function failPendingReceive(rx: RxState, reason: string) {
  if (rx.waiter) {
    const { reject, timer } = rx.waiter;
    clearTimeout(timer);
    rx.waiter = null;
    reject(new Error(reason));
  }
}

function receiveFromRx(rx: RxState, timeoutMs: number): Promise<ConfigMsgOut> {
  const queued = rx.queue.shift();
  if (queued) return Promise.resolve(queued);
  if (rx.waiter) {
    return Promise.reject(
      new Error("Concurrent receive on the same MIDI device"),
    );
  }
  return new Promise<ConfigMsgOut>((resolve, reject) => {
    const timer = setTimeout(() => {
      rx.waiter = null;
      reject(new Error("Timed out waiting for device response"));
    }, timeoutMs);
    rx.waiter = { resolve, reject, timer };
  });
}

function sendFrame(output: MIDIOutput, msg: ConfigMsgIn) {
  const frame = buildConfigFrame(serialize("ConfigMsgIn", msg));
  output.send(Array.from(frame));
}

// Probes an input/output pairing with GetVersion. The pair that answers with
// a Version message is the config cable — port names are only used to order
// candidates, never to decide.
async function probePair(
  input: MIDIInput,
  output: MIDIOutput,
): Promise<string | null> {
  const rx = attachInput(input);
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
  // Prefer names hinting at the config cable (port 2), but probe all pairs
  return candidates.sort((a, b) => {
    const rank = (port: T) => (/config|2/i.test(port.name ?? "") ? 0 : 1);
    return rank(a) - rank(b);
  });
}

async function findDevice(access: MIDIAccess): Promise<FpMidiDevice | null> {
  const inputs = portCandidates(access.inputs.values());
  const outputs = portCandidates(access.outputs.values());

  for (const output of outputs) {
    for (const input of inputs) {
      const version = await probePair(input, output);
      if (version === null) continue;

      const rx = attachInput(input);
      const device: FpMidiDevice = { access, input, output, version, rx };
      access.onstatechange = (event: MIDIConnectionEvent) => {
        const port = event.port;
        if (
          port &&
          (port.id === input.id || port.id === output.id) &&
          port.state === "disconnected"
        ) {
          failPendingReceive(rx, "MIDI device disconnected");
        }
      };
      return device;
    }
  }

  return null;
}

export async function connectToFaderPunk(): Promise<FpMidiDevice> {
  if (!navigator.requestMIDIAccess) {
    throw new Error("Web MIDI is not supported in this browser");
  }
  const access = await navigator.requestMIDIAccess({ sysex: true });
  const device = await findDevice(access);
  if (!device) {
    throw new Error("No Faderpunk MIDI device found");
  }
  return device;
}

export async function tryAutoConnect(): Promise<FpMidiDevice | null> {
  if (!navigator.requestMIDIAccess) return null;

  try {
    // Avoid a surprise permission prompt on page load where the Permissions
    // API can tell us ahead of time.
    if (navigator.permissions?.query) {
      try {
        const status = await navigator.permissions.query({
          name: "midi",
          sysex: true,
        } as PermissionDescriptor);
        if (status.state === "denied") return null;
      } catch {
        // Permissions API may not know "midi" — fall through and try anyway
      }
    }
    const access = await navigator.requestMIDIAccess({ sysex: true });
    return await findDevice(access);
  } catch (error) {
    console.error("Auto-connect failed:", error);
    return null;
  }
}

export async function sendMessage(
  device: FpMidiDevice,
  msg: ConfigMsgIn,
): Promise<void> {
  sendFrame(device.output, msg);
}

export async function receiveMessage(
  device: FpMidiDevice,
): Promise<ConfigMsgOut> {
  return receiveFromRx(device.rx, RECEIVE_TIMEOUT_MS);
}

export async function sendAndReceive(
  device: FpMidiDevice,
  msg: ConfigMsgIn,
): Promise<ConfigMsgOut> {
  await sendMessage(device, msg);
  return receiveMessage(device);
}

export async function receiveBatchMessages(
  device: FpMidiDevice,
  count: bigint,
): Promise<ConfigMsgOut[]> {
  const results: ConfigMsgOut[] = [];

  for (let i = 0n; i < count; i++) {
    results.push(await receiveMessage(device));
  }

  const endMessage = await receiveMessage(device);

  if (endMessage.tag !== "BatchMsgEnd") {
    throw new Error("Expected BatchMsgEnd but received: " + endMessage.tag);
  }

  return results;
}

export function getDeviceName(device: FpMidiDevice): string {
  return `${device.output.manufacturer ?? "ATOV"} ${device.output.name ?? "Faderpunk"}`;
}

export function getDeviceVersion(device: FpMidiDevice): string {
  return device.version;
}
