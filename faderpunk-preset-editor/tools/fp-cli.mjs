#!/usr/bin/env node
// CLI harness for the Faderpunk config-over-SysEx protocol.
// Talks to the device via `sendmidi` / `receivemidi` (Homebrew) so spawn bugs
// can be reproduced and bisected without a browser.
//
// Usage: node tools/fp-cli.mjs <scenario>
// Scenarios: version | gg2 | gg2-solo | controls

import { spawn, execFileSync } from "node:child_process";
import { serialize, deserialize } from "../vendor/fp-config/index.js";
import { buildConfigFrame, parseConfigFrame } from "../lib/sysex.js";

// FW sends config replies on cable 0 → macOS port "Faderpunk";
// requests go to the dedicated config cable (port "Faderpunk Config").
const RX_PORT = "Faderpunk";
const TX_PORT = "Faderpunk Config";

function ts() {
  return new Date().toISOString().slice(11, 23);
}
function log(...args) {
  console.log(`[${ts()}]`, ...args);
}

// ---- RX: persistent receivemidi, reassemble hex sysex lines ----------------
const rxQueue = [];
let rxBuf = "";
const rx = spawn("receivemidi", ["dev", RX_PORT]);
rx.stdout.on("data", (chunk) => {
  rxBuf += chunk.toString();
  let idx;
  while ((idx = rxBuf.indexOf("\n")) >= 0) {
    const line = rxBuf.slice(0, idx).trim();
    rxBuf = rxBuf.slice(idx + 1);
    handleRxLine(line);
  }
});
rx.stderr.on("data", (d) => console.error("receivemidi:", d.toString().trim()));
rx.on("exit", (code) => {
  console.error(`receivemidi exited (${code})`);
});

function handleRxLine(line) {
  // receivemidi: "system-exclusive hex 7D 46 50 01 ... dec" (payload without F0/F7)
  const m = line.match(/system-exclusive\s+(?:hex\s+)?([0-9A-Fa-f ]+?)(?:\s+dec)?$/);
  if (!m) return;
  const bytes = m[1]
    .trim()
    .split(/\s+/)
    .map((h) => parseInt(h, 16));
  const frame = new Uint8Array([0xf0, ...bytes, 0xf7]);
  const payload = parseConfigFrame(frame);
  if (!payload) {
    log("RX: unparseable frame", bytes.length, "bytes");
    return;
  }
  try {
    const msg = deserialize("ConfigMsgOut", payload).value;
    rxQueue.push(msg);
  } catch (e) {
    log("RX: deserialize failed:", e.message || e);
  }
}

function drainRx() {
  rxQueue.length = 0;
}

async function recv(expectedTag, timeoutMs = 2500) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    while (rxQueue.length) {
      const msg = rxQueue.shift();
      if (msg.tag === expectedTag) return msg;
      log(`  ↷ skip stray ${msg.tag} (want ${expectedTag})`);
    }
    await sleep(25);
  }
  throw new Error(`timeout waiting for ${expectedTag}`);
}

// ---- TX ---------------------------------------------------------------------
function send(msg) {
  const frame = buildConfigFrame(serialize("ConfigMsgIn", msg));
  // sendmidi syx wants the inner bytes (no F0/F7), decimal or hex with `hex`
  const inner = Array.from(frame.slice(1, -1)).map((b) =>
    b.toString(16).padStart(2, "0"),
  );
  execFileSync("sendmidi", ["dev", TX_PORT, "hex", "syx", ...inner]);
}

async function request(msg, expectedTag, timeoutMs = 2500) {
  drainRx();
  send(msg);
  return recv(expectedTag, timeoutMs);
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

// ---- protocol helpers ---------------------------------------------------------
async function getVersion(timeoutMs = 2000) {
  const v = await request({ tag: "GetVersion" }, "Version", timeoutMs);
  return `${v.value.major}.${v.value.minor}.${v.value.patch}`;
}

/** slots: array of [appId, channels, layoutId] placed at startChannel index */
function makeLayout(placed) {
  const arr = Array.from({ length: 16 }, () => undefined);
  for (const { ch, appId, channels, id } of placed) {
    arr[ch] = [appId, channels, id];
  }
  return [arr];
}

async function setLayout(placed, label) {
  log(`SetLayout ${label} (${placed.length} apps) …`);
  await request({ tag: "SetLayout", value: makeLayout(placed) }, "Layout", 4000);
  log("  ✓ Layout ack");
}

async function probeAlive(label, timeoutMs = 1500) {
  try {
    const v = await getVersion(timeoutMs);
    log(`  ✓ alive (${label}) fw ${v}`);
    return true;
  } catch {
    log(`  ✗ QUIET (${label})`);
    return false;
  }
}

async function waitSlotReady(id, timeoutMs = 20000) {
  log(`wait layoutId=${id} ready …`);
  const deadline = Date.now() + timeoutMs;
  let last = "no reply";
  while (Date.now() < deadline) {
    try {
      drainRx();
      send({ tag: "GetAppParams", value: { layout_id: id } });
      const deadline2 = Date.now() + 5000;
      let st = null;
      while (Date.now() < deadline2) {
        while (rxQueue.length) {
          const msg = rxQueue.shift();
          if (msg.tag === "AppState" && Number(msg.value[0]) === id) {
            st = msg;
            break;
          }
          log(`  ↷ skip ${msg.tag}${msg.tag === "AppState" ? ` (id ${msg.value[0]})` : ""}`);
        }
        if (st) break;
        await sleep(25);
      }
      if (!st) throw new Error("timeout waiting AppState");
      const n = Array.isArray(st.value[1]) ? st.value[1].length : 0;
      if (n > 0) {
        log(`  ✓ layoutId=${id} ready (${n} params)`);
        return true;
      }
      last = "empty AppState";
    } catch (e) {
      last = e.message || String(e);
      const ok = await probeAlive(`resync id=${id}`);
      if (!ok) throw new Error(`cable quiet while waiting slot ${id}`);
    }
    await sleep(200);
  }
  throw new Error(`layoutId=${id} not ready (${last})`);
}

async function hold() {
  await request({ tag: "HoldPerfMute" }, "Pong", 3000);
  log("HoldPerfMute ✓");
}
async function release() {
  await request({ tag: "ReleasePerfMute" }, "Pong", 5000);
  log("ReleasePerfMute ✓");
}

// App IDs (apps/mod.rs)
const CONTROL = 1;
const LFO_PLUS = 22;
const BERNOULLI = 27;
const HEAT_PUMP = 29;
const GROOVES = 30;
const GG = 31; // Golden Gate (fibonacci_gate), 1 channel
const SUPER_LFO = 32;
const ECHOLOT = 33;

// Break Science layout: ch → [appId, channels, layoutId]
const BREAK_SCIENCE = [
  { ch: 0, appId: GROOVES, channels: 1, id: 0 },
  { ch: 1, appId: BERNOULLI, channels: 2, id: 1 },
  { ch: 3, appId: GG, channels: 1, id: 2 },
  { ch: 4, appId: GG, channels: 1, id: 3 },
  { ch: 5, appId: ECHOLOT, channels: 1, id: 4 },
  { ch: 6, appId: LFO_PLUS, channels: 2, id: 5 },
  { ch: 8, appId: HEAT_PUMP, channels: 1, id: 6 },
  { ch: 9, appId: SUPER_LFO, channels: 2, id: 7 },
  { ch: 11, appId: CONTROL, channels: 1, id: 8 },
  { ch: 12, appId: CONTROL, channels: 1, id: 9 },
  { ch: 13, appId: CONTROL, channels: 1, id: 10 },
  { ch: 14, appId: CONTROL, channels: 1, id: 11 },
  { ch: 15, appId: CONTROL, channels: 1, id: 12 },
];

// ---- scenarios ---------------------------------------------------------------
const scenarios = {
  async version() {
    log("fw", await getVersion());
  },

  // Reproduce: clear → GG@ch3 → GG@ch4 (the editor failure)
  async gg2() {
    log("fw", await getVersion());
    await hold();
    try {
      await setLayout([], "clear");
      await sleep(1500);
      await probeAlive("after clear");

      await setLayout([{ ch: 3, appId: GG, channels: 1, id: 2 }], "GG#1@ch3");
      await sleep(600);
      await waitSlotReady(2);

      await setLayout(
        [
          { ch: 3, appId: GG, channels: 1, id: 2 },
          { ch: 4, appId: GG, channels: 1, id: 3 },
        ],
        "GG#1+GG#2",
      );
      await sleep(600);
      await probeAlive("after GG#2 SetLayout");
      await waitSlotReady(3);
      log("SUCCESS: both Golden Gates ready");
    } finally {
      await release().catch(() => log("release failed"));
    }
  },

  // Full Break Science, incremental like the editor (stop at first failure)
  async breakscience() {
    log("fw", await getVersion());
    await hold();
    try {
      await setLayout([], "clear");
      await sleep(1500);
      await probeAlive("after clear");
      const placed = [];
      for (let i = 0; i < BREAK_SCIENCE.length; i++) {
        const slot = BREAK_SCIENCE[i];
        placed.push(slot);
        await setLayout([...placed], `${i + 1}/${BREAK_SCIENCE.length} app=${slot.appId}@ch${slot.ch}`);
        await sleep(600);
        await waitSlotReady(slot.id);
        await sleep(250);
      }
      log("SUCCESS: all Break Science slots ready");
    } finally {
      await release().catch(() => log("release failed"));
    }
  },

  // Variant: two GGs in one SetLayout after clear
  async gg2solo() {
    log("fw", await getVersion());
    await hold();
    try {
      await setLayout([], "clear");
      await sleep(1500);
      await setLayout(
        [
          { ch: 3, appId: GG, channels: 1, id: 2 },
          { ch: 4, appId: GG, channels: 1, id: 3 },
        ],
        "2×GG atomic",
      );
      await sleep(1500);
      await probeAlive("after atomic 2×GG");
      await waitSlotReady(2);
      await waitSlotReady(3);
      log("SUCCESS: atomic 2×GG ready");
    } finally {
      await release().catch(() => log("release failed"));
    }
  },
};

// custom: node tools/fp-cli.mjs custom "30:0:1" "27:1:2" — appId:ch:channels
scenarios.custom = async function custom() {
  const specs = process.argv.slice(3).map((s, i) => {
    const [appId, ch, channels] = s.split(":").map(Number);
    return { appId, ch, channels: channels || 1, id: i };
  });
  log("fw", await getVersion());
  await hold();
  try {
    await setLayout([], "clear");
    await sleep(1500);
    await probeAlive("after clear");
    const placed = [];
    for (const slot of specs) {
      placed.push(slot);
      await setLayout([...placed], `+app=${slot.appId}@ch${slot.ch}`);
      await sleep(600);
      await waitSlotReady(slot.id);
      await sleep(250);
    }
    log("SUCCESS: all slots ready");
  } finally {
    await release().catch(() => log("release failed"));
  }
};

const name = (process.argv[2] || "version").replace(/-/g, "");
const fn = scenarios[name];
if (!fn) {
  console.error("unknown scenario; use:", Object.keys(scenarios).join(" | "));
  process.exit(2);
}
fn()
  .then(() => {
    log("done");
    process.exit(0);
  })
  .catch((e) => {
    log("FAILED:", e.message || e);
    process.exit(1);
  });
