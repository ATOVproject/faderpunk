import { deserialize, serialize } from "@atov/fp-config";
import { buildConfigFrame, parseConfigFrame, SYSEX_EOX, SYSEX_START } from "./sysex.js";

const RECEIVE_TIMEOUT_MS = 2000;
const PROBE_TIMEOUT_MS = 300;

function attachConfigInput(input) {
  const rx = {
    sysexBuffer: [],
    collecting: false,
    queue: [],
    waiter: null,
  };

  input.onmidimessage = (event) => {
    if (!event.data || event.data.length === 0) return;
    const data = event.data;
    const first = data[0];

    if (first !== SYSEX_START) {
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
        let msg;
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

function receiveFromRx(rx, timeoutMs) {
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

function sendFrame(output, msg) {
  output.send(Array.from(buildConfigFrame(serialize("ConfigMsgIn", msg))));
}

async function probePair(input, output) {
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

function portCandidates(ports) {
  const candidates = Array.from(ports).filter((port) =>
    /faderpunk/i.test(`${port.manufacturer ?? ""} ${port.name ?? ""}`),
  );
  return candidates.sort((a, b) => {
    const rank = (port) => (/config|2/i.test(port.name ?? "") ? 0 : 1);
    return rank(a) - rank(b);
  });
}

/** True if Web MIDI still lists any Faderpunk ports (config or perf). */
export function faderpunkPortsListed(access) {
  if (!access) return false;
  return (
    portCandidates(access.inputs.values()).length > 0 &&
    portCandidates(access.outputs.values()).length > 0
  );
}

/** Connect to Faderpunk config SysEx port (Web MIDI). */
export async function connectDevice() {
  if (!navigator.requestMIDIAccess) {
    throw new Error("Web MIDI is not supported in this browser");
  }
  const access = await navigator.requestMIDIAccess({ sysex: true });
  const inputs = portCandidates(access.inputs.values());
  const outputs = portCandidates(access.outputs.values());

  let config = null;
  const sawPorts = inputs.length > 0 && outputs.length > 0;
  for (const output of outputs) {
    for (const input of inputs) {
      const version = await probePair(input, output);
      if (version === null) continue;
      const rx = attachConfigInput(input);
      await input.open();
      await output.open();
      config = { input, output, version, rx };
      break;
    }
    if (config) break;
  }

  if (!config) {
    throw new Error(
      sawPorts
        ? "Faderpunk MIDI ports present but GetVersion failed (device busy or USB wedged). Wait / replug; close other tabs using the device."
        : "No Faderpunk config MIDI port found. Plug in USB, allow MIDI/SysEx, close other tabs using the device.",
    );
  }

  const inNames = inputs.map((i) => i.name ?? i.id).join(", ");
  const outNames = outputs.map((o) => o.name ?? o.id).join(", ");
  return {
    access,
    config,
    portSummary: `in[${inNames}] · out[${outNames}]`,
  };
}

export function disconnectDevice(device) {
  if (!device?.config) return;
  device.config.input.onmidimessage = null;
  if (device.config.rx?.waiter) {
    clearTimeout(device.config.rx.waiter.timer);
    device.config.rx.waiter = null;
  }
}

/** Drop unmatched config replies so the next request/response pair stays aligned. */
export function drainConfigQueue(rx) {
  if (rx?.queue) rx.queue.length = 0;
}

export async function sendAndReceive(config, msg) {
  sendFrame(config.output, msg);
  return receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS);
}

/**
 * Like sendAndReceive, but keep reading until `expectedTag` (or timeout).
 * Skips stray Layout/AppState from Diagnostics soft-poll or late acks.
 * When `matchLayoutId` is set and expecting AppState, also skip AppStates
 * for other layout slots (stale reply from previous Set/Get).
 */
export async function sendAndReceiveExpect(config, msg, expectedTag, opts = {}) {
  const log = opts.onLog || (() => {});
  const attempts = opts.attempts ?? 8;
  const timeoutMs = opts.timeoutMs ?? RECEIVE_TIMEOUT_MS;
  const matchLayoutId =
    opts.matchLayoutId == null ? null : Number(opts.matchLayoutId);
  drainConfigQueue(config.rx);
  sendFrame(config.output, msg);
  const deadline = Date.now() + timeoutMs * Math.max(2, Math.ceil(attempts / 2));
  let lastTag = null;
  while (Date.now() < deadline) {
    const remaining = Math.max(200, deadline - Date.now());
    let response;
    try {
      response = await receiveFromRx(config.rx, Math.min(timeoutMs, remaining));
    } catch (e) {
      if (lastTag) {
        throw new Error(
          `Expected ${expectedTag}, last stray was ${lastTag} (${e.message || e})`,
        );
      }
      throw e;
    }
    if (response.tag === expectedTag) {
      if (
        matchLayoutId != null &&
        expectedTag === "AppState" &&
        Number(response.value?.[0]) !== matchLayoutId
      ) {
        log(
          `  ↷ skip AppState layoutId=${response.value?.[0]} (want ${matchLayoutId})`,
        );
        continue;
      }
      return response;
    }
    lastTag = response.tag;
    log(`  ↷ skip stray ${response.tag} (want ${expectedTag})`);
  }
  throw new Error(
    `Expected ${expectedTag}, got ${lastTag ?? "timeout"} — close Diagnostics/Configurator on the config MIDI cable`,
  );
}

export async function sendMessage(config, msg) {
  sendFrame(config.output, msg);
}

export async function receiveBatchMessages(config, count) {
  const results = [];
  for (let i = 0n; i < count; i++) {
    results.push(await receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS));
  }
  const endMessage = await receiveFromRx(config.rx, RECEIVE_TIMEOUT_MS);
  if (endMessage.tag !== "BatchMsgEnd") {
    throw new Error("Expected BatchMsgEnd but received: " + endMessage.tag);
  }
  return results;
}
