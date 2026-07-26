import {
  connectDevice,
  disconnectDevice,
  drainConfigQueue,
  receiveBatchMessages,
  sendAndReceive,
  sendAndReceiveExpect,
  sendMessage,
} from "./device.js";
import { serialize } from "@atov/fp-config";

/** Dense playground layouts need longer than stock 1s to spawn param handlers. */
const LAYOUT_SETTLE_MS = 2500;
const SET_PARAMS_RETRIES = 3;

function delay(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function asU8(n) {
  const v = typeof n === "bigint" ? Number(n) : Number(n);
  if (!Number.isFinite(v)) return 0;
  return Math.max(0, Math.min(255, Math.round(v)));
}

/** JSON-safe clone (postcard may yield BigInt). */
function toPlainJson(value) {
  return JSON.parse(
    JSON.stringify(value, (_k, v) => (typeof v === "bigint" ? Number(v) : v)),
  );
}

/** Coerce editor/JSON quirks into postcard Value shapes. */
function normalizeValueForWire(v) {
  if (!v || typeof v !== "object" || !("tag" in v)) return v;
  const tag = v.tag;
  // i32/f32/Enum/bool are scalars — never single-element arrays
  if (
    (tag === "i32" || tag === "f32" || tag === "Enum" || tag === "bool") &&
    Array.isArray(v.value)
  ) {
    return { tag, value: v.value[0] };
  }
  // MidiOut must be [[usb,out1,out2]] not [usb,out1,out2]
  if (tag === "MidiOut" && Array.isArray(v.value) && v.value.length === 3 && typeof v.value[0] === "boolean") {
    return { tag, value: [v.value] };
  }
  return v;
}

function padParams(values) {
  const result = Array.from({ length: 16 }, () => undefined);
  (values || []).forEach((v, i) => {
    if (i < 16) result[i] = normalizeValueForWire(v);
  });
  return result;
}

async function getAllApps(config, log) {
  log?.("GetAllApps…");
  const response = await sendAndReceive(config, { tag: "GetAllApps" });
  if (response.tag !== "BatchMsgStart") {
    throw new Error(`GetAllApps failed: ${response.tag}`);
  }
  const apps = await receiveBatchMessages(config, response.value);
  const map = new Map();
  for (const item of apps) {
    if (item.tag !== "AppConfig") continue;
    const [appId, channels, meta] = item.value;
    map.set(appId, {
      appId,
      channels,
      paramCount: meta[0],
      name: meta[1],
      description: meta[2],
      color: meta[3],
      icon: meta[4],
      params: meta[5],
    });
  }
  log?.(`  → ${map.size} apps`);
  return map;
}

function transformLayout(layoutMsg, allApps) {
  const layout = [];
  let lastUsed = -1;
  let nextEmptyId = 16;
  layoutMsg.value[0].forEach((slot, idx) => {
    if (idx <= lastUsed) return;
    if (!slot) {
      lastUsed++;
      layout.push({ id: nextEmptyId++, app: null, startChannel: idx });
      return;
    }
    const [appId, channels, layoutId] = slot;
    const app = allApps.get(appId);
    if (!app) {
      lastUsed++;
      layout.push({ id: nextEmptyId++, app: null, startChannel: idx });
      return;
    }
    lastUsed = idx + Number(channels) - 1;
    layout.push({ id: layoutId, app, startChannel: idx });
  });
  return layout;
}

function toLayoutFile(appLayout, paramsById, globalConfig, description) {
  return {
    version: 1,
    description,
    layout: appLayout.map(({ id, app, startChannel }) => ({
      layoutId: id,
      appId: !app?.appId || !paramsById.has(id) ? null : app.appId,
      startChannel,
      params: paramsById.get(id) || null,
    })),
    config: globalConfig,
  };
}

/**
 * Pull layout + params + global config from the device over SysEx.
 * @param {{ onLog?: (line: string) => void }} [opts]
 * @returns {Promise<{ setup: object, version: string, portSummary: string, ms: number }>}
 */
export async function pullSetupFromDevice(opts = {}) {
  const log = opts.onLog || (() => {});
  const t0 = Date.now();
  let device;
  try {
    log("Connecting via Web MIDI…");
    device = await connectDevice();
    log(`Connected · fw ${device.config.version} · ${device.portSummary}`);
    const { config } = device;

    const allApps = await getAllApps(config, log);

    log("GetLayout…");
    const layoutResponse = await sendAndReceive(config, { tag: "GetLayout" });
    if (layoutResponse.tag !== "Layout") {
      throw new Error(`GetLayout failed: ${layoutResponse.tag}`);
    }
    const appLayout = transformLayout(layoutResponse, allApps);
    const appSlots = appLayout.filter((s) => s.app);
    log(`  → ${appSlots.length} app slot(s)`);

    log("GetAllAppParams…");
    const paramsResponse = await sendAndReceive(config, { tag: "GetAllAppParams" });
    if (paramsResponse.tag !== "BatchMsgStart") {
      throw new Error(`GetAllAppParams failed: ${paramsResponse.tag}`);
    }
    const paramMsgs = await receiveBatchMessages(config, paramsResponse.value);
    const paramsById = new Map();
    for (const item of paramMsgs) {
      if (item.tag !== "AppState") continue;
      const [layoutId, values] = item.value;
      paramsById.set(layoutId, values);
    }
    log(`  → ${paramsById.size} param set(s)`);

    log("GetGlobalConfig…");
    const gcResponse = await sendAndReceive(config, { tag: "GetGlobalConfig" });
    if (gcResponse.tag !== "GlobalConfig") {
      throw new Error(`GetGlobalConfig failed: ${gcResponse.tag}`);
    }

    const setup = toPlainJson(
      toLayoutFile(
        appLayout,
        paramsById,
        gcResponse.value,
        `Pulled ${new Date().toISOString()}`,
      ),
    );

    const ms = Date.now() - t0;
    log(`Pull done · ${(ms / 1000).toFixed(1)}s`);
    return { setup, version: device.config.version, portSummary: device.portSummary, ms };
  } finally {
    disconnectDevice(device);
  }
}

/**
 * Push a LayoutFile (version 1) to the device — same sequence as Configurator RecallSetup.
 * @param {object} setup
 * @param {{ onLog?: (line: string) => void }} [opts]
 */
export async function pushSetupToDevice(setup, opts = {}) {
  const log = opts.onLog || (() => {});
  const t0 = Date.now();
  if (!setup?.layout || !Array.isArray(setup.layout)) {
    throw new Error("Invalid setup: missing layout[]");
  }

  let device;
  try {
    log("Connecting via Web MIDI…");
    device = await connectDevice();
    log(`Connected · fw ${device.config.version} · ${device.portSummary}`);
    const { config } = device;

    const allApps = await getAllApps(config, log);

    const appLayout = [];
    const paramsById = new Map();
    for (const slot of setup.layout) {
      const { appId, layoutId, params, startChannel } = slot;
      if (!appId) {
        appLayout.push({ id: layoutId, app: null, startChannel });
        continue;
      }
      const app = allApps.get(appId);
      if (!app || !params) {
        log(`  ⚠ skip layoutId ${layoutId}: unknown app ${appId} or no params`);
        appLayout.push({ id: layoutId, app: null, startChannel });
        continue;
      }
      appLayout.push({ id: layoutId, app, startChannel });
      paramsById.set(layoutId, params);
    }

    const active = appLayout.filter((s) => s.app).length;
    log(`SetLayout (${active} apps)…`);
    const sendLayout = [
      Array.from({ length: 16 }, () => undefined),
    ];
    let currentChan = 0;
    for (const appSlot of appLayout) {
      if (currentChan >= 16) break;
      if (appSlot.app) {
        sendLayout[0][currentChan] = [
          appSlot.app.appId,
          appSlot.app.channels,
          appSlot.id,
        ];
        currentChan += Number(appSlot.app.channels);
      } else {
        currentChan++;
      }
    }

    const layoutAck = await sendAndReceiveExpect(
      config,
      { tag: "SetLayout", value: sendLayout },
      "Layout",
      { onLog: log },
    );
    if (layoutAck.tag !== "Layout") {
      throw new Error(`SetLayout failed: ${layoutAck.tag}`);
    }

    log(`  settle ${LAYOUT_SETTLE_MS}ms (apps spawning)…`);
    await delay(LAYOUT_SETTLE_MS);

    log(`SetAppParams (${paramsById.size})…`);
    for (const [layoutId, values] of paramsById) {
      const id = Number(layoutId);
      let lastErr = null;
      for (let attempt = 0; attempt < SET_PARAMS_RETRIES; attempt++) {
        try {
          drainConfigQueue(config.rx);
          const response = await sendAndReceiveExpect(
            config,
            {
              tag: "SetAppParams",
              value: {
                layout_id: id,
                values: padParams(values),
              },
            },
            "AppState",
            { onLog: log, timeoutMs: 3000 },
          );
          if (asU8(response.value[0]) !== asU8(id)) {
            throw new Error(
              `SetAppParams layout_id mismatch: sent ${id}, got ${response.value[0]}`,
            );
          }
          lastErr = null;
          break;
        } catch (e) {
          lastErr = e;
          log(
            `  ⚠ SetAppParams(${id}) attempt ${attempt + 1}/${SET_PARAMS_RETRIES}: ${e.message || e}`,
          );
          await delay(400 + attempt * 300);
        }
      }
      if (lastErr) {
        const padded = padParams(values);
        for (let i = 0; i < padded.length; i++) {
          if (padded[i] == null) continue;
          const one = Array.from({ length: 16 }, () => undefined);
          one[i] = padded[i];
          try {
            serialize("ConfigMsgIn", {
              tag: "SetAppParams",
              value: { layout_id: id, values: one },
            });
          } catch {
            throw new Error(
              `SetAppParams layoutId=${id} param[${i}] ${JSON.stringify(padded[i])}: ${lastErr.message || lastErr}`,
            );
          }
        }
        throw new Error(
          `SetAppParams(layoutId=${id}): ${lastErr.message || lastErr}\n` +
            `Close MIDI Diagnostics / other Configurator tabs (shared SysEx cable), then retry Push.`,
        );
      }
    }

    if (setup.config) {
      log("SetGlobalConfig…");
      // Firmware does not ack SetGlobalConfig — fire-and-forget + brief settle.
      await sendMessage(config, {
        tag: "SetGlobalConfig",
        value: setup.config,
      });
      await delay(200);
    }

    const ms = Date.now() - t0;
    log(`Push done · ${(ms / 1000).toFixed(1)}s`);
    return { ok: true, version: device.config.version, portSummary: device.portSummary, ms };
  } finally {
    disconnectDevice(device);
  }
}
