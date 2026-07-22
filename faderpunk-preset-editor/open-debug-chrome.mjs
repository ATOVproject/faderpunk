/**
 * Start (or reuse) Chrome for editor + local Configurator.
 * Tab 1: http://127.0.0.1:3847/  Tab 2: http://127.0.0.1:5173/#/configurator
 * Never kills an existing WebMIDI session; never starts a second profile Chrome.
 */
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync, spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROFILE = join(__dirname, ".chrome-profile");
const EDITOR_URL = process.env.FP_EDITOR_URL || "http://127.0.0.1:3847/";
import { resolveConfigUrl } from "./config-url.mjs";
// Resolved at startup in main(): local :5173 preferred, hosted beta fallback.
let CONFIG_URL = "http://127.0.0.1:5173/#/configurator";
const CDP_PORTS = [
  Number(process.env.FP_CDP_PORT) || 9223,
  9222,
  9224,
  9225,
];
const CHROME =
  process.env.FP_CHROME ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

function profileChromePids() {
  try {
    const out = execSync(
      "pgrep -f 'Google Chrome.*faderpunk-scenes/.chrome-profile' || true",
      { encoding: "utf8" },
    );
    return out
      .split(/\s+/)
      .map((s) => s.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

async function tryConnect(port) {
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const context = browser.contexts()[0];
  if (!context) throw new Error("no context");
  return { browser, context, port };
}

async function connectAny() {
  for (const port of CDP_PORTS) {
    try {
      return await tryConnect(port);
    } catch {
      /* next */
    }
  }
  return null;
}

async function ensureTab(context, urlTest, gotoUrl, label) {
  for (const page of context.pages()) {
    try {
      if (urlTest(page.url())) {
        console.log(`  → Tab ok: ${label} (${page.url()})`);
        await page.bringToFront().catch(() => {});
        return page;
      }
    } catch {
      /* detached */
    }
  }
  console.log(`  → Opening tab: ${label}`);
  const page = await context.newPage();
  await page.goto(gotoUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  return page;
}

async function prepareTabs(context) {
  // Editor first (homepage), then local Configurator
  const editor = await ensureTab(
    context,
    (u) => /127\.0\.0\.1:3847|localhost:3847/i.test(u),
    EDITOR_URL,
    "Editor",
  );
  // Find a tab already on the resolved configurator; else migrate any
  // configurator tab (local or hosted) to the resolved URL.
  const configOriginRe = /faderpunk\.io|127\.0\.0\.1:5173|localhost:5173/i;
  const onResolved = (u) => {
    try {
      return u.startsWith(new URL(CONFIG_URL).origin) && /#\/configurator/i.test(u);
    } catch {
      return false;
    }
  };
  let config = null;
  for (const page of context.pages()) {
    try {
      const u = page.url();
      if (onResolved(u)) {
        console.log(`  → Tab ok: Configurator (${u})`);
        await page.bringToFront().catch(() => {});
        config = page;
        break;
      }
    } catch {
      /* detached */
    }
  }
  if (!config) {
    for (const page of context.pages()) {
      try {
        const u = page.url();
        if (configOriginRe.test(u)) {
          console.log(`  → Switching Configurator tab: ${u} → ${CONFIG_URL}`);
          await page.goto(CONFIG_URL, {
            waitUntil: "domcontentloaded",
            timeout: 60000,
          });
          config = page;
          break;
        }
      } catch {
        /* detached */
      }
    }
  }
  if (!config) {
    config = await ensureTab(context, onResolved, CONFIG_URL, "Configurator");
  }
  await editor.bringToFront().catch(() => {});
}

async function launchFresh(port) {
  if (profileChromePids().length) {
    throw new Error(
      "Configurator Chrome is already running without CDP.\n" +
        "Close the window and try the button again — or start manually with --remote-debugging-port=9223.",
    );
  }
  await mkdir(PROFILE, { recursive: true });
  console.log(`Starting Local Configurator :${port}…`);
  spawn(
    CHROME,
    [
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${PROFILE}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-blink-features=AutomationControlled",
      EDITOR_URL,
      CONFIG_URL,
    ],
    { detached: true, stdio: "ignore" },
  ).unref();

  for (let i = 0; i < 40; i++) {
    await sleep(250);
    try {
      const session = await tryConnect(port);
      await prepareTabs(session.context);
      console.log(`OK — Local Configurator on CDP ${port}`);
      console.log(`  Config: ${CONFIG_URL}`);
      console.log(`  Editor-Tab (optional): ${EDITOR_URL}`);
      console.log("Configurator: Connect Device — keep Push/Pull in the editor.");
      return { ok: true, port, launched: true };
    } catch {
      /* wait */
    }
  }
  throw new Error(`Chrome gestartet, CDP :${port} antwortet nicht.`);
}

async function main() {
  const resolved = await resolveConfigUrl();
  CONFIG_URL = resolved.url;
  console.log(`Configurator: ${CONFIG_URL} [${resolved.source}]`);
  console.log("Local Configurator: looking for CDP…");
  let session = await connectAny();
  if (!session) {
    for (let i = 0; i < 6 && !session; i++) {
      await sleep(300);
      session = await connectAny();
    }
  }

  if (session) {
    console.log(`CDP :${session.port} — checking tabs…`);
    await prepareTabs(session.context);
    console.log("OK — existing Configurator Chrome, tabs ready.");
    console.log("Connect Device in Configurator — keep Push/Pull in the editor.");
    return { ok: true, port: session.port, launched: false };
  }

  return launchFresh(CDP_PORTS[0]);
}

main()
  .then((r) => {
    if (r && r.ok) process.exit(0);
  })
  .catch((e) => {
    console.error(e.message || e);
    process.exit(1);
  });
