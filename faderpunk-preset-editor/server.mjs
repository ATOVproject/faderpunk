import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join, extname } from "node:path";
import { fileURLToPath } from "node:url";
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
        res.end(JSON.stringify({ ok: false, error: String(e.message || e) }));
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
        res.end(JSON.stringify({ ok: false, error: String(e.message || e) }));
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
        "Cache-Control":
          extname(path) === ".js" || extname(path) === ".html"
            ? "no-store"
            : "public, max-age=60",
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
  console.log(`Pull/Push: Web MIDI SysEx in the browser (no Configurator / CDP)`);
  console.log(`Bank: GET|PUT /api/bank`);
  console.log(`Catalog: GET /api/catalog  CCs: GET /api/ccs?path=Nord/Drum%203P.csv`);
  console.log(
    `MIDI DB: GET /api/midi/status  POST /api/midi/sync  POST /api/midi/upload  (custom: ${MIDI_CUSTOM_DIR})`,
  );
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
