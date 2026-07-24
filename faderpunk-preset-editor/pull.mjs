/**
 * Pull current device Setup from Configurator (Save Setup) via Playwright CDP.
 * Attaches to the EXISTING Chrome — never kills / never starts a second instance
 * while a profile Chrome is already open (WebMIDI = one connection only).
 */
import { chromium } from "playwright";
import { mkdir, writeFile, readFile } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync, spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, "out");
const PULL_PATH = join(OUT_DIR, "pulled-setup.json");
const PROFILE = join(__dirname, ".chrome-profile");
import { resolveConfigUrl, configOrigin } from "./config-url.mjs";
// Resolved at startup in main(): local :5173 preferred, hosted beta fallback.
let CONFIG_URL = "http://127.0.0.1:5173/#/configurator";
let CONFIG_ORIGIN = configOrigin(CONFIG_URL);
const CDP_PORTS = [
  Number(process.env.FP_CDP_PORT) || 9223,
  9222,
  9224,
  9225,
];
const CHROME =
  process.env.FP_CHROME ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

function isLocalConfigurator(u) {
  try {
    return (
      new URL(u).origin === CONFIG_ORIGIN && /#\/configurator/i.test(u)
    );
  } catch {
    return false;
  }
}

async function findFaderpunkPage(browser) {
  let fallback = null;
  for (const context of browser.contexts()) {
    for (const page of context.pages()) {
      try {
        const u = page.url();
        if (isLocalConfigurator(u)) return { context, page };
        if (/faderpunk\.io|127\.0\.0\.1:5173|localhost:5173/i.test(u) && !fallback)
          fallback = { context, page };
      } catch {
        /* detached */
      }
    }
  }
  return fallback;
}

async function tryConnectCdp(port) {
  const endpoint = `http://127.0.0.1:${port}`;
  const browser = await chromium.connectOverCDP(endpoint);
  const found = await findFaderpunkPage(browser);
  const context = found?.context || browser.contexts()[0] || (await browser.newContext());
  const page = found?.page || (await context.newPage());
  console.log(
    `CDP ${endpoint}` +
      (found ? ` → Tab ${page.url()}` : " (no Faderpunk tab — opening Configurator)"),
  );
  return { browser, page, context };
}

async function connectAnyCdp() {
  for (const port of CDP_PORTS) {
    try {
      return await tryConnectCdp(port);
    } catch {
      /* next */
    }
  }
  return null;
}

function profileChromePids() {
  try {
    const out = execSync(
      `pgrep -f 'Google Chrome.*${PROFILE}' || true`,
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

async function waitForCdp(port, tries = 40) {
  for (let i = 0; i < tries; i++) {
    try {
      return await tryConnectCdp(port);
    } catch {
      await sleep(250);
    }
  }
  throw new Error(`Chrome CDP on port ${port} not reachable.`);
}

/** Only when no profile Chrome is running — never kill an existing connected browser. */
async function launchProfileWithCdp(port) {
  if (profileChromePids().length) {
    throw new Error(
      "Push Chrome is already running, but remote debugging is not responding.\n" +
        "Do not open a second Chrome (WebMIDI is single-owner).\n\n" +
        "Fix: close that Chrome, then pull again — or start Chrome like this:\n" +
        `  "${CHROME}" --remote-debugging-port=${port} --user-data-dir="${PROFILE}"`,
    );
  }
  await mkdir(PROFILE, { recursive: true });
  console.log(`Starting Chrome profile with CDP :${port}…`);
  spawn(
    CHROME,
    [
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${PROFILE}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-blink-features=AutomationControlled",
      CONFIG_URL,
    ],
    { detached: true, stdio: "ignore" },
  ).unref();
  return waitForCdp(port);
}

async function ensureConnected(page) {
  const url = page.url();
  const onSelected = (() => {
    try {
      return (
        new URL(url).origin === CONFIG_ORIGIN && /#\/configurator/i.test(url)
      );
    } catch {
      return false;
    }
  })();
  if (!onSelected) {
    console.log(`[1/3] Opening selected Configurator (${CONFIG_URL})…`);
    await page.goto(CONFIG_URL, { waitUntil: "domcontentloaded", timeout: 60000 });
    await page.waitForTimeout(1000);
  } else {
    console.log(`[1/3] Reusing selected Configurator tab (${CONFIG_ORIGIN}).`);
  }

  console.log("[2/3] Settings tab…");
  const settingsTab = page.getByRole("tab", { name: /settings/i }).or(
    page.locator("text=Settings").first(),
  );
  if (await settingsTab.count()) {
    await settingsTab.first().click();
    await page.waitForTimeout(400);
  }

  const connectBtn = page.getByRole("button", { name: /connect device/i });
  if (await connectBtn.isVisible().catch(() => false)) {
    console.log("[2/3] Waiting for Connect Device (up to 120s)…");
    await connectBtn.click().catch(() => {});
    await page
      .waitForFunction(
        () =>
          !Array.from(document.querySelectorAll("button")).some((b) =>
            /connect device/i.test(b.textContent || ""),
          ),
        null,
        { timeout: 120000 },
      )
      .catch(() => {
        throw new Error(
          "Timeout: Faderpunk not connected. Click Connect Device in the same Chrome.",
        );
      });
    await page.waitForTimeout(1500);
  } else {
    console.log("[2/3] Device already connected.");
  }
}

async function saveSetupFromPage(page) {
  console.log("[3/3] Save current Setup (Download)…");

  const saveBtn = page.getByRole("button", { name: /Save current Setup/i });
  await saveBtn.waitFor({ state: "visible", timeout: 20000 }).catch(() => {
    throw new Error(
      '"Save current Setup" not visible — Settings tab, layout with apps?',
    );
  });

  const [download] = await Promise.all([
    page.waitForEvent("download", { timeout: 30000 }),
    saveBtn.click(),
  ]);

  await mkdir(OUT_DIR, { recursive: true });
  const tmp = join(OUT_DIR, "pulled-setup.download.json");
  await download.saveAs(tmp);

  const text = await readFile(tmp, "utf8");
  const setup = JSON.parse(text);
  if (!setup?.layout || !Array.isArray(setup.layout)) {
    throw new Error("Download is not a valid setup (layout[] missing).");
  }
  await writeFile(PULL_PATH, JSON.stringify(setup, null, 2), "utf8");
  console.log(`OK — ${PULL_PATH}`);
  console.log(
    `  slots: ${setup.layout.filter((s) => s.appId).length} apps, config: ${setup.config ? "yes" : "no"}`,
  );
  return setup;
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true });

  const resolved = await resolveConfigUrl();
  CONFIG_URL = resolved.url;
  CONFIG_ORIGIN = configOrigin(CONFIG_URL);
  console.log(
    `Configurator: ${CONFIG_URL} [${resolved.source}] prefer=${process.env.FP_CONFIG_PREFER || "auto"}`,
  );

  console.log("Looking for CDP (existing Chrome, no kill)…");
  let session = await connectAnyCdp();

  if (!session) {
    // Retry briefly — profile Chrome may still be starting
    for (let i = 0; i < 8 && !session; i++) {
      await sleep(300);
      session = await connectAnyCdp();
    }
  }

  if (!session) {
    if (profileChromePids().length) {
      throw new Error(
        "Configurator Chrome is running without remote debugging — pull cannot attach.\n" +
          "Do not open a second window (WebMIDI would block).\n\n" +
          "Once: close that Chrome, then “Read from device” (starts debug Chrome),\n" +
          "or manually:\n" +
          `  "${CHROME}" --remote-debugging-port=9223 --user-data-dir="${PROFILE}" "${CONFIG_URL}"`,
      );
    }
    console.log("No CDP — starting debug Chrome once…");
    session = await launchProfileWithCdp(CDP_PORTS[0]);
  }

  try {
    // WebMIDI-SysEx permission per CDP (Chrome prompt is hard-blocked in this profile)
    await session.context
      ?.grantPermissions(["midi", "midi-sysex"], { origin: CONFIG_ORIGIN })
      .catch(() => {});
    await ensureConnected(session.page);
    await saveSetupFromPage(session.page);
    console.log("Done — editor will take the preset. Chrome left open.");
  } finally {
    // CDP trennen — sonst bleibt der Pull-Prozess hängen und die UI disabled.
    await session?.browser?.close().catch(() => {});
  }
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error(e.message || e);
    process.exit(1);
  });
