import {
  connectDevice,
  disconnectDevice,
  drainConfigQueue,
  receiveBatchMessages,
  sendAndReceive,
  sendAndReceiveExpect,
  sendMessage,
} from "./device.js";

/** Live app swap / row edit — one (or few) slots respawn, shorter settle. */
const LAYOUT_SETTLE_LIVE_MS = 3500;
/** Incremental Full Push: one new app per SetLayout — short settle + cable poll. */
const LAYOUT_SETTLE_INCREMENTAL_MS = 2000;
const SET_PARAMS_RETRIES = 4;
/** Pause after SetAppParams: firmware respawns the app (param_handler exits). */
const SET_PARAMS_GAP_MS = 900;
/** Host wait per attempt. Firmware may spend up to ~8s in FRAM before AppState. */
const SET_PARAMS_TIMEOUT_MS = 15000;

function delay(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/**
 * Sleep without going SysEx-silent for long stretches — macOS/Web MIDI can
 * drop the config port after ~1min of idle during a long Full Push.
 */
async function delayKeepalive(config, ms) {
  if (ms <= 0) return;
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const left = end - Date.now();
    const slice = Math.min(left, 1200);
    await delay(slice);
    if (Date.now() >= end) break;
    try {
      await probeConfigCable(config);
    } catch {
      // spawn may still be muting replies briefly — ignore until settle
    }
  }
}

/**
 * Mild settle scaling by layout size (not per-app).
 */
function settleMsForLayout(appCount, baseMs, { capMs = null } = {}) {
  let ms = Math.max(baseMs, Number(appCount) * 550);
  if (capMs != null) ms = Math.min(ms, capMs);
  return ms;
}

/** Stable log / params order: channel ascending. Firmware spawns by channel. */
function compareChannelOrder(a, b) {
  return (Number(a.startChannel) || 0) - (Number(b.startChannel) || 0);
}

/** Place apps at final startChannel (holes OK). Falls back to packed order. */
function buildSendLayout(appLayout) {
  const sendLayout = [Array.from({ length: 16 }, () => undefined)];
  const placed = appLayout.filter((s) => s.app);
  const useStart =
    placed.length > 0 &&
    placed.every((s) => Number.isFinite(Number(s.startChannel)));
  if (useStart) {
    for (const slot of placed) {
      const ch = Number(slot.startChannel);
      if (ch < 0 || ch >= 16) continue;
      sendLayout[0][ch] = [
        slot.app.appId,
        slot.app.channels,
        slot.id,
      ];
    }
    return sendLayout;
  }
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
  return sendLayout;
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

/**
 * Cheap cable check — if this times out, SetLayout likely killed USB MIDI
 * or another tab holds the config port.
 */
async function assertConfigCableAlive(config, log) {
  drainConfigQueue(config.rx);
  try {
    const response = await sendAndReceiveExpect(
      config,
      { tag: "GetVersion" },
      "Version",
      { onLog: log, timeoutMs: 2500, attempts: 4 },
    );
    if (response.tag !== "Version") {
      throw new Error(`expected Version, got ${response.tag}`);
    }
  } catch (e) {
    throw new Error(
      `Config MIDI quiet after layout settle (${e.message || e}).\n` +
        `Close Scopepunk / Configurator tabs, replug USB if the device vanished.`,
    );
  }
}

/** Single short GetVersion — for settle polling (no log spam). */
async function probeConfigCable(config) {
  drainConfigQueue(config.rx);
  const response = await sendAndReceiveExpect(
    config,
    { tag: "GetVersion" },
    "Version",
    { timeoutMs: 600, attempts: 2 },
  );
  if (response.tag !== "Version") {
    throw new Error(`expected Version, got ${response.tag}`);
  }
}

/**
 * GetVersion works during LAYOUT_USB_MIDI_MUTE; SetAppParams does not (empty
 * AppState). Poll GetAppParams until the slot actually has a param_handler.
 */
async function waitForSlotReady(config, layoutId, log, timeoutMs = 12000) {
  const id = Number(layoutId);
  log(`  wait layoutId=${id} ready (not muted / spawned) …`);
  const deadline = Date.now() + timeoutMs;
  let lastDetail = "no reply";
  while (Date.now() < deadline) {
    try {
      drainConfigQueue(config.rx);
      // Host wait must exceed FW GetAppParams timeout under Hold (400ms) and
      // the non-hold 3s path — otherwise we desync SysEx and never recover.
      const response = await sendAndReceiveExpect(
        config,
        { tag: "GetAppParams", value: { layout_id: id } },
        "AppState",
        { timeoutMs: 5000, attempts: 2, matchLayoutId: id },
      );
      const n = Array.isArray(response.value?.[1])
        ? response.value[1].length
        : Array.isArray(response.value?.values)
          ? response.value.values.length
          : 0;
      if (n > 0) {
        log(`  ✓ layoutId=${id} ready (${n} params)`);
        return config;
      }
      lastDetail = "empty AppState (mute or not spawned)";
    } catch (e) {
      lastDetail = e.message || String(e);
      // Resync probe — if Version works, keep polling; if not, surface cable death.
      try {
        await probeConfigCable(config);
      } catch (cableErr) {
        throw new Error(
          `layoutId=${id} not ready; config cable quiet (${cableErr.message || cableErr})`,
        );
      }
    }
    await delay(200);
  }
  throw new Error(
    `layoutId=${id} not ready after SetLayout (${lastDetail})`,
  );
}

/**
 * SetLayout + settle + cable check. Poll GetVersion until spawn storm settles
 * (fixed sleep alone races dense layouts). On cable death, one reconnect retry.
 * @param {string} [label] optional log label e.g. "SetLayout (3/13)"
 */
async function applySetLayout(
  config,
  appLayout,
  log,
  settleMs,
  deviceRef,
  label,
  settleOpts = {},
) {
  const active = appLayout.filter((s) => s.app).length;
  log(`${label || `SetLayout (${active} apps)`} …`);
  const sendLayout = buildSendLayout(appLayout);

  const layoutAck = await sendAndReceiveExpect(
    config,
    { tag: "SetLayout", value: sendLayout },
    "Layout",
    { onLog: log },
  );
  if (layoutAck.tag !== "Layout") {
    throw new Error(`SetLayout failed: ${layoutAck.tag}`);
  }

  const headStart = settleOpts.headStartMs ?? 800;
  const pollBudget =
    settleOpts.pollBudgetMs ??
    Math.max(2500, settleMsForLayout(active, settleMs, settleOpts));
  // Head-start must not consume the poll window (that caused false "cable quiet").
  const totalMs = headStart + pollBudget;
  log(
    `  settle ${totalMs}ms (head ${headStart}ms + poll ${pollBudget}ms) …`,
  );
  let alive = false;
  let lastErr = null;
  // Keepalive during long heads (atomic 13-app spawn ~30s) so Web MIDI
  // does not drop the config port mid-wait.
  await delayKeepalive(config, headStart);
  const deadline = Date.now() + pollBudget;
  while (Date.now() < deadline) {
    try {
      await probeConfigCable(config);
      alive = true;
      break;
    } catch (e) {
      lastErr = e;
      await delay(350);
    }
  }
  if (alive) {
    log("  ✓ config cable alive");
    return config;
  }
  log(`  ⚠ ${lastErr?.message || lastErr || "cable quiet after settle"}`);
  if (!deviceRef) throw lastErr || new Error("config cable quiet after settle");
  log("  retry: reconnect Web MIDI …");
  disconnectDevice(deviceRef.device);
  await delay(800);
  deviceRef.device = await connectDevice();
  log(
    `  reconnected · fw ${deviceRef.device.config.version} · ${deviceRef.device.portSummary}`,
  );
  await assertConfigCableAlive(deviceRef.device.config, log);
  log("  ✓ config cable alive after reconnect");
  return deviceRef.device.config;
}

/**
 * Full Push under HoldPerfMute — true one-app-at-a-time SetLayout.
 *
 * Hold → clear → for each app: SetLayout(prefix) + wait slot → SetAppParams
 * → Release (FW store + unmute). Never kicks a deferred multi-app spawn burst.
 */
async function applySetLayoutIncremental(
  config,
  appLayout,
  paramsById,
  log,
  deviceRef,
) {
  const activeSlots = appLayout.filter((s) => s.app);
  const n = activeSlots.length;
  if (n === 0) {
    log("SetLayout (0 apps) …");
    return config;
  }
  const ordered = [...activeSlots].sort(compareChannelOrder);
  log(
    `Incremental SetLayout under Hold (${n} apps): ${ordered
      .map((s) => `${s.app?.name || s.app?.appId}(ch${Number(s.startChannel) || 0})`)
      .join(" → ")}`,
  );
  let cfg = config;
  let holdingMute = false;

  try {
    await sendAndReceiveExpect(
      cfg,
      { tag: "HoldPerfMute" },
      "Pong",
      { onLog: log, timeoutMs: 2500, attempts: 3 },
    );
    holdingMute = true;
    log("  HoldPerfMute (mute Local MIDI; spawn per SetLayout)");

    cfg = await applySetLayout(
      cfg,
      [],
      log,
      LAYOUT_SETTLE_INCREMENTAL_MS,
      deviceRef,
      "SetLayout (clear)",
      { headStartMs: 300, pollBudgetMs: 1500 },
    );

    const growing = [];
    for (let i = 0; i < ordered.length; i++) {
      growing.push(ordered[i]);
      const slot = ordered[i];
      const label = `${slot.app?.name || slot.app?.appId}(ch${Number(slot.startChannel) || 0})`;
      cfg = await applySetLayout(
        cfg,
        growing,
        log,
        LAYOUT_SETTLE_INCREMENTAL_MS,
        deviceRef,
        `SetLayout (${i + 1}/${n}) ${label}`,
        { headStartMs: 400, pollBudgetMs: 2500 },
      );
      // Confirm this slot's param_handler is up before adding the next app.
      cfg = await waitForSlotReady(cfg, slot.id, log, 25_000);
      await delay(250);
    }

    const ids = ordered.map((s) => Number(s.id));
    log("  SetAppParams (all) …");
    await applySetAppParams(cfg, paramsById, ids, log);

    await sendAndReceiveExpect(
      cfg,
      { tag: "ReleasePerfMute" },
      "Pong",
      { onLog: log, timeoutMs: 5000, attempts: 3 },
    );
    holdingMute = false;
    log("  ReleasePerfMute (store + unmute)");
    return cfg;
  } catch (e) {
    if (holdingMute) {
      try {
        await sendAndReceiveExpect(
          cfg,
          { tag: "ReleasePerfMute" },
          "Pong",
          { timeoutMs: 5000, attempts: 1 },
        );
      } catch {
        /* ignore */
      }
    }
    throw e;
  }
}

/** Poll GetAppParams until every layoutId has a live param_handler. */
async function waitForAllSlotsReady(
  config,
  layoutIds,
  log,
  timeoutMs,
  deviceRef,
) {
  const pending = new Set(layoutIds.map(Number));
  const deadline = Date.now() + timeoutMs;
  log(`  wait ${pending.size} slots ready (up to ${timeoutMs}ms) …`);
  let cfg = config;
  let lastProgress = Date.now();
  let reconnects = 0;
  const maxReconnects = 2;
  while (pending.size > 0 && Date.now() < deadline) {
    let progressed = false;
    for (const id of [...pending]) {
      try {
        drainConfigQueue(cfg.rx);
        const response = await sendAndReceiveExpect(
          cfg,
          { tag: "GetAppParams", value: { layout_id: id } },
          "AppState",
          { timeoutMs: 1200, attempts: 1 },
        );
        const values = response.value?.[1] ?? response.value?.values;
        if (Array.isArray(values) && values.length > 0) {
          pending.delete(id);
          progressed = true;
          lastProgress = Date.now();
          log(`  ✓ layoutId=${id} ready (${values.length} params)`);
        }
      } catch {
        /* still spawning / quiet */
      }
      await delay(40);
    }
    if (pending.size === 0) break;
    // Some slots came up, then nothing for a while → likely TaskPool
    // exhausted (remaining layoutIds were never spawned). Fail fast
    // instead of hammering GetAppParams until the config cable dies.
    const stalledMs = Date.now() - lastProgress;
    const ready = layoutIds.length - pending.size;
    if (ready > 0 && stalledMs > 8_000) {
      throw new Error(
        `slots stalled: ${ready}/${layoutIds.length} ready, still missing ${[...pending].join(",")} — often Embassy pool_size exhausted`,
      );
    }
    try {
      await probeConfigCable(cfg);
    } catch (e) {
      if (!deviceRef) throw e;
      reconnects += 1;
      if (reconnects > maxReconnects) {
        throw new Error(
          `config cable quiet while waiting slots (missing ${[...pending].join(",")}); gave up after ${maxReconnects} reconnects`,
        );
      }
      log("  ⚠ cable quiet while waiting slots — reconnect …");
      disconnectDevice(deviceRef.device);
      await delay(800);
      try {
        deviceRef.device = await connectDevice();
        cfg = deviceRef.device.config;
        log(
          `  reconnected · fw ${deviceRef.device.config.version} · ${deviceRef.device.portSummary}`,
        );
      } catch (re) {
        throw new Error(
          `USB lost waiting slots ${[...pending].join(",")} (${re.message || re})`,
        );
      }
    }
    await delay(400);
  }
  if (pending.size > 0) {
    throw new Error(
      `slots not ready: ${[...pending].join(",")} after ${timeoutMs}ms`,
    );
  }
  return cfg;
}

function buildAppLayout(setup, allApps, log) {
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
  return { appLayout, paramsById };
}

async function applySetAppParams(config, paramsById, layoutIds, log) {
  const ids =
    layoutIds == null
      ? [...paramsById.keys()].map(Number)
      : layoutIds.map(Number);

  async function setOne(id) {
    const values = paramsById.get(id) ?? paramsById.get(String(id));
    if (!values) {
      log(`  ⚠ skip layoutId=${id}: no params in setup`);
      return;
    }
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
          { onLog: log, timeoutMs: SET_PARAMS_TIMEOUT_MS, matchLayoutId: id },
        );
        const nvals = Array.isArray(response.value[1])
          ? response.value[1].length
          : 0;
        if (nvals === 0) {
          throw new Error(
            `SetAppParams(${id}): empty AppState (slot not running — pool/spawn?)`,
          );
        }
        log(`  ✓ layoutId=${id} (${nvals} params)`);
        lastErr = null;
        break;
      } catch (e) {
        lastErr = e;
        log(
          `  ⚠ SetAppParams(${id}) attempt ${attempt + 1}/${SET_PARAMS_RETRIES}: ${e.message || e}`,
        );
        const wait = String(e.message || e).includes("empty AppState")
          ? 2500
          : 1000 + attempt * 600;
        await delay(wait);
      }
    }
    if (lastErr) {
      throw new Error(
        `SetAppParams(layoutId=${id}): ${lastErr.message || lastErr}\n` +
          `Close Scopepunk / other Configurator tabs (shared SysEx cable), then retry Push.`,
      );
    }
    await delay(SET_PARAMS_GAP_MS);
  }

  log(`SetAppParams (${ids.length}) …`);
  for (const id of ids) {
    await setOne(id);
  }
  try {
    await assertConfigCableAlive(config, log);
    log("  ✓ config cable alive after params");
  } catch (e) {
    log(`  ⚠ ${e.message || e}`);
  }
}

async function getAllApps(config, log) {
  log?.("GetAllApps …");
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
    log("Connecting via Web MIDI …");
    device = await connectDevice();
    log(`Connected · fw ${device.config.version} · ${device.portSummary}`);
    const { config } = device;

    const allApps = await getAllApps(config, log);

    log("GetLayout …");
    const layoutResponse = await sendAndReceive(config, { tag: "GetLayout" });
    if (layoutResponse.tag !== "Layout") {
      throw new Error(`GetLayout failed: ${layoutResponse.tag}`);
    }
    const appLayout = transformLayout(layoutResponse, allApps);
    const appSlots = appLayout.filter((s) => s.app);
    log(`  → ${appSlots.length} app slot(s)`);

    log("GetAllAppParams …");
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

    log("GetGlobalConfig …");
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
    log("Connecting via Web MIDI …");
    device = await connectDevice();
    log(`Connected · fw ${device.config.version} · ${device.portSummary}`);
    const deviceRef = { device };

    const allApps = await getAllApps(deviceRef.device.config, log);
    const { appLayout, paramsById } = buildAppLayout(setup, allApps, log);
    let config = await applySetLayoutIncremental(
      deviceRef.device.config,
      appLayout,
      paramsById,
      log,
      deviceRef,
    );
    device = deviceRef.device;

    if (setup.config) {
      log("SetGlobalConfig …");
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

/**
 * Live structural push: SetLayout + SetAppParams for selected (or all) slots.
 * Used when swapping an app / adding / reordering — not a param-only tweak.
 *
 * @param {object} setup
 * @param {{
 *   onLog?: (line: string) => void,
 *   paramLayoutIds?: number[] | null,
 *   settleMs?: number,
 * }} [opts]
 */
export async function pushLiveStructureToDevice(setup, opts = {}) {
  const log = opts.onLog || (() => {});
  const settleMs = opts.settleMs ?? LAYOUT_SETTLE_LIVE_MS;
  const t0 = Date.now();
  if (!setup?.layout || !Array.isArray(setup.layout)) {
    throw new Error("Invalid setup: missing layout[]");
  }

  let device;
  try {
    log("Connecting via Web MIDI …");
    device = await connectDevice();
    log(`Connected · fw ${device.config.version} · ${device.portSummary}`);
    const deviceRef = { device };
    const allApps = await getAllApps(deviceRef.device.config, log);
    const { appLayout, paramsById } = buildAppLayout(setup, allApps, log);
    const config = await applySetLayout(
      deviceRef.device.config,
      appLayout,
      log,
      settleMs,
      deviceRef,
    );
    device = deviceRef.device;
    const ids =
      opts.paramLayoutIds == null
        ? null
        : opts.paramLayoutIds.map(Number).filter((n) => Number.isFinite(n));
    await applySetAppParams(config, paramsById, ids, log);
    const ms = Date.now() - t0;
    log(`Live structure done · ${(ms / 1000).toFixed(1)}s`);
    return { ok: true, ms, version: device.config.version };
  } finally {
    if (device) disconnectDevice(device);
  }
}

/**
 * Push params for a single layout_id only (no SetLayout).
 * Use after the device layout already matches the editor (one full Push first).
 */
export async function pushAppParamsToDevice(layoutId, values, opts = {}) {
  const log = opts.onLog || (() => {});
  const id = Number(layoutId);
  if (!Number.isFinite(id) || id < 0 || id > 15) {
    throw new Error(`Invalid layoutId ${layoutId}`);
  }
  const padded = padParams(values);
  let device;
  try {
    device = await connectDevice();
    const { config } = device;
    drainConfigQueue(config.rx);
    log(`Live SetAppParams(layoutId=${id}) …`);
    let lastErr = null;
    for (let attempt = 0; attempt < SET_PARAMS_RETRIES; attempt++) {
      try {
        drainConfigQueue(config.rx);
        const response = await sendAndReceiveExpect(
          config,
          {
            tag: "SetAppParams",
            value: { layout_id: id, values: padded },
          },
          "AppState",
          { onLog: log, timeoutMs: SET_PARAMS_TIMEOUT_MS, matchLayoutId: id },
        );
        const n = Array.isArray(response.value[1]) ? response.value[1].length : 0;
        if (n === 0) {
          throw new Error(
            `empty AppState — slot ${id} not running (spawn/pool?). Full Push layout first.`,
          );
        }
        log(`  ✓ layoutId=${id} (${n} params)`);
        return { ok: true, layoutId: id, n, version: device.config.version };
      } catch (e) {
        lastErr = e;
        log(
          `  ⚠ live SetAppParams(${id}) attempt ${attempt + 1}/${SET_PARAMS_RETRIES}: ${e.message || e}`,
        );
        await delay(400 + attempt * 300);
      }
    }
    throw lastErr || new Error(`SetAppParams(${id}) failed`);
  } finally {
    if (device) disconnectDevice(device);
  }
}
