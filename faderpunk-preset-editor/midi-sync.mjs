/**
 * Bootstrap / update the Pencil Research MIDI CC & NRPN CSV database
 * (https://github.com/pencilresearch/midi → midi.guide).
 *
 * Upstream lives in ./midi (auto-downloaded). User overlays in ./midi-custom
 * are never overwritten.
 */
import {
  mkdir,
  readdir,
  readFile,
  writeFile,
  rm,
  rename,
  access,
} from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));

export const MIDI_DIR = join(__dirname, "midi");
export const MIDI_CUSTOM_DIR = join(__dirname, "midi-custom");
export const MIDI_LEGACY_DIR = join(__dirname, "midi-main");
export const MIDI_META = join(MIDI_DIR, ".source.json");

const REPO = "pencilresearch/midi";
const ZIP_URL = `https://codeload.github.com/${REPO}/zip/refs/heads/main`;
const SHA_URL = `https://api.github.com/repos/${REPO}/commits/main`;

async function pathExists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

export async function walkCsv(dir, base = dir, out = []) {
  if (!(await pathExists(dir))) return out;
  const entries = await readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    if (e.name.startsWith(".")) continue;
    const full = join(dir, e.name);
    if (e.isDirectory()) {
      await walkCsv(full, base, out);
    } else if (e.name.toLowerCase().endsWith(".csv")) {
      out.push(
        full
          .slice(base.length)
          .replace(/^[/\\]/, "")
          .replaceAll("\\", "/"),
      );
    }
  }
  return out;
}

export async function countCsv(dir) {
  return (await walkCsv(dir)).length;
}

async function run(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { ...opts, stdio: ["ignore", "pipe", "pipe"] });
    let err = "";
    child.stderr.on("data", (d) => {
      err += d;
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${cmd} ${args.join(" ")} failed (${code}): ${err.trim()}`));
    });
  });
}

export async function fetchRemoteSha() {
  const res = await fetch(SHA_URL, {
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": "faderpunk-preset-editor-midi-sync",
    },
  });
  if (!res.ok) throw new Error(`GitHub SHA lookup failed (${res.status})`);
  const data = await res.json();
  return String(data.sha || "").slice(0, 40);
}

export async function readLocalMeta() {
  try {
    return JSON.parse(await readFile(MIDI_META, "utf8"));
  } catch {
    return null;
  }
}

async function downloadZip(destZip) {
  const res = await fetch(ZIP_URL, {
    headers: { "User-Agent": "faderpunk-preset-editor-midi-sync" },
  });
  if (!res.ok || !res.body) {
    throw new Error(`Download failed (${res.status})`);
  }
  await pipeline(res.body, createWriteStream(destZip));
}

/**
 * Replace ./midi with a fresh extract of pencilresearch/midi@main.
 * Preserves nothing inside ./midi (custom CSVs live in ./midi-custom).
 */
export async function syncMidiFromGithub({ force = false } = {}) {
  await mkdir(MIDI_DIR, { recursive: true });
  await mkdir(MIDI_CUSTOM_DIR, { recursive: true });

  const remoteSha = await fetchRemoteSha();
  const local = await readLocalMeta();
  const localCount = await countCsv(MIDI_DIR);

  if (!force && local?.sha === remoteSha && localCount > 0) {
    return {
      ok: true,
      updated: false,
      sha: remoteSha,
      count: localCount,
      message: "Already up to date",
    };
  }

  const stamp = Date.now();
  const work = join(tmpdir(), `fp-midi-${stamp}`);
  const zipPath = join(work, "midi.zip");
  await mkdir(work, { recursive: true });

  try {
    await downloadZip(zipPath);
    if (process.platform === "darwin") {
      // ditto handles non-ASCII zip paths better than unzip(1) on macOS
      await run("ditto", ["-x", "-k", zipPath, work]);
    } else {
      await run("unzip", ["-q", "-o", zipPath, "-d", work]);
    }
    // GitHub zip root is usually "midi-main"
    const entries = await readdir(work, { withFileTypes: true });
    const rootDir = entries.find(
      (e) => e.isDirectory() && e.name.toLowerCase().startsWith("midi"),
    );
    if (!rootDir) throw new Error("Zip extract: midi folder not found");
    const extracted = join(work, rootDir.name);

    const staging = join(__dirname, `.midi-staging-${stamp}`);
    await rm(staging, { recursive: true, force: true });
    await run("cp", ["-R", extracted, staging]);

    // Swap into place
    const backup = join(__dirname, `.midi-backup-${stamp}`);
    if (await pathExists(MIDI_DIR)) {
      await rename(MIDI_DIR, backup);
    }
    try {
      await rename(staging, MIDI_DIR);
      await writeFile(
        MIDI_META,
        JSON.stringify(
          {
            sha: remoteSha,
            repo: REPO,
            syncedAt: new Date().toISOString(),
            source: ZIP_URL,
          },
          null,
          2,
        ),
        "utf8",
      );
      await rm(backup, { recursive: true, force: true });
    } catch (e) {
      // rollback
      await rm(MIDI_DIR, { recursive: true, force: true }).catch(() => {});
      if (await pathExists(backup)) await rename(backup, MIDI_DIR);
      throw e;
    }

    const count = await countCsv(MIDI_DIR);
    return {
      ok: true,
      updated: true,
      sha: remoteSha,
      count,
      message: `Synced ${count} CSVs from ${REPO}@${remoteSha.slice(0, 7)}`,
    };
  } finally {
    await rm(work, { recursive: true, force: true }).catch(() => {});
  }
}

/**
 * Prefer ./midi; fall back to legacy midi-main symlink; otherwise download.
 */
export async function ensureMidiCatalog() {
  await mkdir(MIDI_CUSTOM_DIR, { recursive: true });

  const midiCount = await countCsv(MIDI_DIR);
  if (midiCount > 0) {
    const meta = await readLocalMeta();
    return {
      root: MIDI_DIR,
      count: midiCount,
      customCount: await countCsv(MIDI_CUSTOM_DIR),
      sha: meta?.sha || null,
      bootstrapped: false,
      source: "midi",
    };
  }

  const legacyCount = await countCsv(MIDI_LEGACY_DIR);
  if (legacyCount > 0) {
    return {
      root: MIDI_LEGACY_DIR,
      count: legacyCount,
      customCount: await countCsv(MIDI_CUSTOM_DIR),
      sha: null,
      bootstrapped: false,
      source: "midi-main (legacy)",
    };
  }

  console.log("MIDI CC database missing — downloading pencilresearch/midi…");
  const result = await syncMidiFromGithub({ force: true });
  return {
    root: MIDI_DIR,
    count: result.count,
    customCount: await countCsv(MIDI_CUSTOM_DIR),
    sha: result.sha,
    bootstrapped: true,
    source: "github",
    message: result.message,
  };
}

export async function midiStatus() {
  const meta = await readLocalMeta();
  const midiCount = await countCsv(MIDI_DIR);
  const legacyCount = await countCsv(MIDI_LEGACY_DIR);
  const customCount = await countCsv(MIDI_CUSTOM_DIR);
  let remoteSha = null;
  let updateAvailable = false;
  try {
    remoteSha = await fetchRemoteSha();
    // Encourage sync when still on legacy midi-main (no .source.json) or SHA drifted
    updateAvailable = Boolean(
      remoteSha && (!meta?.sha || remoteSha !== meta.sha),
    );
  } catch {
    /* offline */
  }
  return {
    ok: true,
    repo: REPO,
    site: "https://midi.guide/",
    sha: meta?.sha || null,
    remoteSha,
    syncedAt: meta?.syncedAt || null,
    updateAvailable,
    count: midiCount || legacyCount,
    customCount,
    root:
      midiCount > 0 ? "midi" : legacyCount > 0 ? "midi-main" : "midi (empty)",
    customDir: "midi-custom",
  };
}

/** Resolve a catalog path to an absolute file path. */
export async function resolveCsvPath(rel) {
  if (!rel || rel.includes("..")) throw new Error("bad path");
  const norm = rel.replaceAll("\\", "/");
  if (norm.startsWith("Custom/")) {
    return join(MIDI_CUSTOM_DIR, norm.slice("Custom/".length));
  }
  const inMidi = join(MIDI_DIR, norm);
  if (await pathExists(inMidi)) return inMidi;
  const inLegacy = join(MIDI_LEGACY_DIR, norm);
  if (await pathExists(inLegacy)) return inLegacy;
  throw Object.assign(new Error(`CSV not found: ${norm}`), { code: "ENOENT" });
}

/** Merged catalog: upstream paths + Custom/* overlays. */
export async function listCatalog() {
  const upstreamRoot =
    (await countCsv(MIDI_DIR)) > 0
      ? MIDI_DIR
      : (await countCsv(MIDI_LEGACY_DIR)) > 0
        ? MIDI_LEGACY_DIR
        : MIDI_DIR;
  const upstream = await walkCsv(upstreamRoot);
  const custom = (await walkCsv(MIDI_CUSTOM_DIR)).map((p) => `Custom/${p}`);
  return [...upstream, ...custom].sort((a, b) => a.localeCompare(b));
}

/** Save a user CSV into midi-custom/ (never touches upstream ./midi). */
export async function uploadCustomCsv(filename, text) {
  const base = String(filename || "")
    .split(/[/\\]/)
    .pop()
    ?.trim();
  if (!base || base.startsWith(".")) throw new Error("bad filename");
  if (!base.toLowerCase().endsWith(".csv")) throw new Error("CSV files only");
  if (base.includes("..")) throw new Error("bad filename");
  await mkdir(MIDI_CUSTOM_DIR, { recursive: true });
  const dest = join(MIDI_CUSTOM_DIR, base);
  await writeFile(dest, String(text ?? ""), "utf8");
  const path = `Custom/${base}`;
  return {
    ok: true,
    path,
    message: `Saved ${path}`,
  };
}
