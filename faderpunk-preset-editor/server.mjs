import { createServer } from "node:http";
import { readFile, writeFile, mkdir, readdir } from "node:fs/promises";
import { join, extname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import {
  ensureMidiCatalog,
  listCatalog,
  resolveCsvPath,
  syncMidiFromGithub,
  midiStatus,
  uploadCustomCsv,
  MIDI_CUSTOM_DIR,
} from "./midi-sync.mjs";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const PORT = 3847;
const SETUP_PATH = join(__dirname, "out", "current-setup.json");
const PULL_PATH = join(__dirname, "out", "pulled-setup.json");
const BANK_PATH = join(__dirname, "out", "preset-bank.json");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".md": "text/markdown; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".csv": "text/csv; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".svg": "image/svg+xml",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".ttf": "font/ttf",
};

function runChildScript(scriptName, { timeoutMs = 180_000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [join(__dirname, scriptName)], {
      cwd: __dirname,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let out = "";
    let err = "";
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      child.kill("SIGTERM");
      setTimeout(() => child.kill("SIGKILL"), 2000);
      const log = [out, err].filter(Boolean).join("\n").trim();
      reject(
        Object.assign(
          new Error(`${scriptName} timed out after ${Math.round(timeoutMs / 1000)}s`),
          { log, out, err, code: -1 },
        ),
      );
    }, timeoutMs);
    child.stdout.on("data", (d) => {
      out += d;
    });
    child.stderr.on("data", (d) => {
      err += d;
    });
    child.on("close", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      const log = [out, err].filter(Boolean).join("\n").trim();
      if (code === 0) resolve({ ok: true, out, err, log });
      else
        reject(
          Object.assign(new Error(err || out || `${scriptName} exited ${code}`), {
            log,
            out,
            err,
            code,
          }),
        );
    });
  });
}

/** Stream child stdout/stderr as NDJSON lines, then a final {type:"done"} object. */
function streamChildScriptNdjson(res, scriptName, { timeoutMs = 180_000, t0 = Date.now() } = {}) {
  const child = spawn(process.execPath, [join(__dirname, scriptName)], {
    cwd: __dirname,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let out = "";
  let err = "";
  let settled = false;
  let lineBuf = "";

  const writeMsg = (obj) => {
    try {
      res.write(`${JSON.stringify(obj)}\n`);
    } catch {
      /* client gone */
    }
  };

  const emitChunk = (chunk, stream) => {
    const text = String(chunk);
    if (stream === "out") out += text;
    else err += text;
    lineBuf += text;
    const parts = lineBuf.split(/\r?\n/);
    lineBuf = parts.pop() || "";
    for (const line of parts) {
      const trimmed = line.trim();
      if (trimmed) writeMsg({ type: "log", line: trimmed });
    }
  };

  res.writeHead(200, {
    "Content-Type": "application/x-ndjson; charset=utf-8",
    "Cache-Control": "no-cache",
    "X-Accel-Buffering": "no",
  });

  const timer = setTimeout(() => {
    if (settled) return;
    settled = true;
    child.kill("SIGTERM");
    setTimeout(() => child.kill("SIGKILL"), 2000);
    if (lineBuf.trim()) writeMsg({ type: "log", line: lineBuf.trim() });
    writeMsg({
      type: "done",
      ok: false,
      ms: Date.now() - t0,
      error: `${scriptName} timed out after ${Math.round(timeoutMs / 1000)}s`,
      log: [out, err].filter(Boolean).join("\n").trim(),
    });
    res.end();
  }, timeoutMs);

  child.stdout.on("data", (d) => emitChunk(d, "out"));
  child.stderr.on("data", (d) => emitChunk(d, "err"));
  child.on("close", (code) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    if (lineBuf.trim()) writeMsg({ type: "log", line: lineBuf.trim() });
    const log = [out, err].filter(Boolean).join("\n").trim();
    if (code === 0) {
      writeMsg({ type: "done", ok: true, ms: Date.now() - t0, log });
    } else {
      writeMsg({
        type: "done",
        ok: false,
        ms: Date.now() - t0,
        error: err || out || `${scriptName} exited ${code}`,
        log,
      });
    }
    res.end();
  });
}

async function runPush() {
  return runChildScript("push.mjs");
}

async function runDebugChrome() {
  return runChildScript("open-debug-chrome.mjs", { timeoutMs: 120_000 });
}

async function runPull() {
  return runChildScript("pull.mjs");
}

async function walkCsv(dir, base = dir, out = []) {
  // kept for any leftover callers — prefer listCatalog from midi-sync
  const entries = await readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    const full = join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name.startsWith(".")) continue;
      await walkCsv(full, base, out);
    } else if (e.name.toLowerCase().endsWith(".csv")) {
      out.push(relative(base, full).replaceAll("\\", "/"));
    }
  }
  return out;
}

function parseCsvCcs(text) {
  const lines = text.split(/\r?\n/).filter(Boolean);
  if (!lines.length) return [];
  const headers = lines[0].split(",");
  const idxMsb = headers.indexOf("cc_msb");
  const idxName = headers.indexOf("parameter_name");
  const idxSec = headers.indexOf("section");
  const seen = new Set();
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    // naive CSV split is ok for these midi-main files (quoted fields rare in cc rows)
    const cols = [];
    let cur = "";
    let q = false;
    for (const ch of lines[i]) {
      if (ch === '"') {
        q = !q;
        continue;
      }
      if (ch === "," && !q) {
        cols.push(cur);
        cur = "";
        continue;
      }
      cur += ch;
    }
    cols.push(cur);
    const msb = (cols[idxMsb] || "").trim();
    if (!msb) continue;
    const cc = Number(msb);
    if (!Number.isFinite(cc) || seen.has(cc)) continue;
    seen.add(cc);
    const name = cols[idxName] || `CC${cc}`;
    const sec = cols[idxSec] || "";
    rows.push({ cc, name: sec ? `${sec}: ${name}` : name });
  }
  return rows.sort((a, b) => a.cc - b.cc);
}

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);

    if (req.method === "POST" && url.pathname === "/api/push") {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const body = Buffer.concat(chunks).toString("utf8");
      JSON.parse(body);
      await mkdir(join(__dirname, "out"), { recursive: true });
      await writeFile(SETUP_PATH, body, "utf8");
      const t0 = Date.now();
      streamChildScriptNdjson(res, "push.mjs", { t0 });
      return;
    }

    if (req.method === "POST" && url.pathname === "/api/debug-chrome") {
      const t0 = Date.now();
      try {
        const result = await runDebugChrome();
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            ok: true,
            ms: Date.now() - t0,
            log: result.log || result.out || "",
          }),
        );
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            ok: false,
            ms: Date.now() - t0,
            error: String(e.message || e),
            log: e.log || "",
          }),
        );
      }
      return;
    }

    if (req.method === "POST" && url.pathname === "/api/pull") {
      const t0 = Date.now();
      try {
        const result = await runPull();
        const setup = JSON.parse(await readFile(PULL_PATH, "utf8"));
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            ok: true,
            ms: Date.now() - t0,
            setup,
            log: result.log || result.out || "",
          }),
        );
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            ok: false,
            ms: Date.now() - t0,
            error: String(e.message || e),
            log: e.log || "",
          }),
        );
      }
      return;
    }

    if (req.method === "GET" && url.pathname === "/api/bank") {
      try {
        const raw = await readFile(BANK_PATH, "utf8");
        const data = JSON.parse(raw);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: true, ...data }));
      } catch (e) {
        if (e && e.code === "ENOENT") {
          res.writeHead(404, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ ok: false, error: "no bank yet" }));
          return;
        }
        throw e;
      }
      return;
    }

    if (req.method === "PUT" && url.pathname === "/api/bank") {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const body = Buffer.concat(chunks).toString("utf8");
      const data = JSON.parse(body);
      if (!Array.isArray(data.presets) || data.presets.length < 1) {
        throw new Error("bank needs presets[]");
      }
      await mkdir(join(__dirname, "out"), { recursive: true });
      const savedAt = new Date().toISOString();
      const payload = {
        version: 20,
        savedAt,
        active: Number.isFinite(data.active) ? data.active : 0,
        presets: data.presets,
      };
      await writeFile(BANK_PATH, JSON.stringify(payload, null, 2), "utf8");
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, savedAt, n: payload.presets.length }));
      return;
    }

    if (req.method === "GET" && url.pathname === "/api/catalog") {
      const list = await listCatalog();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(list));
      return;
    }

    if (req.method === "GET" && url.pathname === "/api/midi/status") {
      const status = await midiStatus();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(status));
      return;
    }

    if (req.method === "POST" && url.pathname === "/api/midi/sync") {
      try {
        const result = await syncMidiFromGithub({ force: true });
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify(result));
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({ ok: false, error: String(e.message || e) }),
        );
      }
      return;
    }

    if (req.method === "POST" && url.pathname === "/api/midi/upload") {
      try {
        const name =
          url.searchParams.get("name") ||
          req.headers["x-filename"] ||
          "upload.csv";
        const chunks = [];
        for await (const c of req) chunks.push(c);
        const text = Buffer.concat(chunks).toString("utf8");
        if (!text.trim()) throw new Error("empty file");
        const result = await uploadCustomCsv(String(name), text);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify(result));
      } catch (e) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({ ok: false, error: String(e.message || e) }),
        );
      }
      return;
    }

    if (req.method === "GET" && url.pathname === "/api/ccs") {
      const rel = url.searchParams.get("path") || "";
      if (!rel || rel.includes("..")) throw new Error("bad path");
      const abs = await resolveCsvPath(rel);
      const text = await readFile(abs, "utf8");
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(parseCsvCcs(text)));
      return;
    }

    if (
      (req.method === "GET" || req.method === "HEAD") &&
      (url.pathname === "/" || url.pathname === "/index.html")
    ) {
      const html = await readFile(join(__dirname, "index.html"), "utf8");
      res.writeHead(200, { "Content-Type": MIME[".html"] });
      res.end(req.method === "HEAD" ? undefined : html);
      return;
    }

    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405, { "Content-Type": "text/plain; charset=utf-8" });
      res.end("Method not allowed");
      return;
    }

    const safe = url.pathname.replace(/\.\./g, "");
    const path = join(__dirname, safe);
    try {
      const data = await readFile(path);
      res.writeHead(200, {
        "Content-Type": MIME[extname(path)] || "application/octet-stream",
      });
      res.end(req.method === "HEAD" ? undefined : data);
    } catch (e) {
      if (e && (e.code === "ENOENT" || e.code === "EISDIR")) {
        res.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" });
        res.end(req.method === "HEAD" ? undefined : "Not found");
        return;
      }
      throw e;
    }
  } catch (e) {
    res.writeHead(500, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: false, error: String(e.message || e) }));
  }
});

server.listen(PORT, "127.0.0.1", async () => {
  console.log(`Faderpunk preset editor: http://127.0.0.1:${PORT}/`);
  console.log(`Push: POST /api/push  Pull: POST /api/pull  Bank: GET|PUT /api/bank`);
  console.log(`Catalog: GET /api/catalog  CCs: GET /api/ccs?path=Nord/Drum%203P.csv`);
  console.log(`MIDI DB: GET /api/midi/status  POST /api/midi/sync  POST /api/midi/upload  (custom: ${MIDI_CUSTOM_DIR})`);
  try {
    const info = await ensureMidiCatalog();
    console.log(
      `MIDI CSVs: ${info.count} upstream (${info.source})` +
        (info.customCount ? ` + ${info.customCount} custom` : "") +
        (info.sha ? ` @ ${String(info.sha).slice(0, 7)}` : "") +
        (info.bootstrapped ? " [downloaded]" : ""),
    );
  } catch (e) {
    console.warn(`MIDI catalog bootstrap failed: ${e.message || e}`);
    console.warn("Drop CSVs into midi-custom/ or retry POST /api/midi/sync");
  }
});
