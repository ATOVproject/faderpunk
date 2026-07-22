/**
 * Automates Configurator "Load Setup" in Chrome via Playwright.
 *
 * Order:
 * 1) Connect to existing Chrome with remote debugging (9222 / 9223)
 * 2) If our push-profile Chrome is already open → quit it, then relaunch with CDP
 * 3) Fresh persistent profile + --remote-debugging-port=9223 (so next push reuses it)
 */
import { chromium } from "playwright";
import { readFile, mkdir } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync, spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SETUP_PATH = join(__dirname, "out", "current-setup.json");
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

/** HeroUI Switch = role=switch. Blindes Klicken toggelt und schaltet Defaults AUS. */
async function ensureRecallOn(page, nameRe) {
  const switchEl = page.getByRole("switch", { name: nameRe });
  if (await switchEl.count()) {
    const el = switchEl.first();
    const on =
      (await el.getAttribute("aria-checked")) === "true" ||
      (await el.isChecked().catch(() => false));
    if (!on) {
      await el.click({ force: true });
      console.log(`  → Switch on: ${nameRe}`);
    } else {
      console.log(`  → Switch already on: ${nameRe}`);
    }
    return;
  }
  const cb = page.getByRole("checkbox", { name: nameRe });
  if (await cb.count()) {
    if (!(await cb.first().isChecked().catch(() => false))) {
      await cb.first().check({ force: true });
      console.log(`  → Checkbox on: ${nameRe}`);
    }
    return;
  }
  console.log(`  ⚠ Recall control not found: ${nameRe}`);
}

/** Nach Load: Takeover im Settings-Formular setzen + Save (falls Recall/Config verfehlt). */
async function ensureTakeoverInSettings(page, expectedTag) {
  if (!expectedTag) return;
  console.log(`[5b/5] Checking Takeover in Settings (expected: ${expectedTag})…`);

  await page
    .getByText(/Fader Takeover Mode/i)
    .first()
    .waitFor({ state: "visible", timeout: 8000 })
    .catch(() => {});

  const misc = page.getByText(/^Miscellaneous$/i).first();
  let trigger = misc.locator("xpath=following::button[@aria-haspopup='listbox'][1]");
  if (!(await trigger.count())) {
    trigger = page.getByRole("button", {
      name: /Pickup|Jump|Scale|Select mode|Takeover/i,
    });
  }

  if (!(await trigger.count())) {
    console.log("  ⚠ Takeover select not found — skipped");
    return;
  }

  const current = ((await trigger.first().innerText().catch(() => "")) || "").trim();
  if (new RegExp(`\\b${expectedTag}\\b`, "i").test(current)) {
    console.log(`  → Takeover already “${expectedTag}” (${current})`);
    return;
  }

  console.log(`  → Takeover is “${current || "?"}”, setting ${expectedTag}…`);
  await trigger.first().click({ force: true });
  await page.waitForTimeout(300);
  const option = page.getByRole("option", { name: new RegExp(expectedTag, "i") });
  if (await option.count()) {
    await option.first().click({ force: true });
  } else {
    const byKey = page.locator(`[data-key="${expectedTag}"]`).first();
    if (await byKey.count()) {
      await byKey.click({ force: true });
    } else {
      await page
        .getByText(new RegExp(`^${expectedTag}`, "i"))
        .last()
        .click({ force: true });
    }
  }
  await page.waitForTimeout(200);

  const saveBtn = page.getByRole("button", { name: /^save$/i }).last();
  if (await saveBtn.count()) {
    await saveBtn.click({ force: true });
    await page.waitForTimeout(1200);
    console.log("  → Settings Save (Takeover) sent");
  }
}

function layoutHasMidiCh16(setup) {
  return (setup?.layout || []).some((lay) =>
    (lay.params || []).some(
      (p) => p?.tag === "MidiChannel" && Number(p.value?.[0]) === 16,
    ),
  );
}

/**
 * Soft-reset CoreMIDI when WebMIDI ports go stale after an abrupt disconnect
 * (common on macOS after health-check disconnect mid SetLayout). Often avoids
 * needing a physical USB unplug.
 */
function softResetCoreMidi() {
  try {
    execSync("killall MIDIServer 2>/dev/null || true", { stdio: "ignore" });
    console.log("  → CoreMIDI restarted (MIDIServer)");
    return true;
  } catch {
    return false;
  }
}

async function isOnConfigurator(page) {
  try {
    const u = page.url();
    return new URL(u).origin === CONFIG_ORIGIN && /#\/configurator/i.test(u);
  } catch {
    return false;
  }
}

async function connectDeviceIfNeeded(page, label = "Connect Device") {
  const connectBtn = page.getByRole("button", { name: /connect device/i });
  if (!(await connectBtn.isVisible().catch(() => false))) {
    console.log(`  → ${label}: already connected.`);
    return true;
  }
  console.log(`  → ${label} (WebMIDI, up to 60s)…`);
  await connectBtn.click().catch(() => {});
  try {
    await page.waitForFunction(
      () =>
        !Array.from(document.querySelectorAll("button")).some((b) =>
          /connect device/i.test(b.textContent || ""),
        ),
      null,
      { timeout: 60000 },
    );
    await page.waitForTimeout(600);
    console.log(`  → ${label}: ok.`);
    return true;
  } catch {
    return false;
  }
}

/**
 * After Load Setup the beta health-check (getGlobalConfig every 2s, timeout 2s)
 * often fails while SetLayout is still busy → disconnect() + redirect to "/".
 * Bring the configurator tab back and re-open MIDI.
 */
async function recoverConnectionAfterLoad(page) {
  // Give SetLayout / params / global config a moment; then watch briefly.
  await page.waitForTimeout(2000);

  const checkLost = async () =>
    !(await isOnConfigurator(page)) ||
    (await page
      .getByRole("button", { name: /connect device/i })
      .isVisible()
      .catch(() => false));

  let lost = await checkLost();
  if (!lost) {
    // One more probe after another health-check interval.
    await page.waitForTimeout(2200);
    lost = await checkLost();
  }

  if (!lost) {
    console.log("[5d/5] Connection still up after load.");
    return;
  }

  console.log(
    "[5d/5] Connection lost (typical: beta health-check timeout during SetLayout) — reconnecting…",
  );

  if (!(await isOnConfigurator(page))) {
    await page.goto(CONFIG_URL, {
      waitUntil: "domcontentloaded",
      timeout: 60000,
    });
    await page.waitForTimeout(800);
  }

  let ok = await connectDeviceIfNeeded(page, "Reconnect");
  if (!ok) {
    console.log("  → Reconnect failed — CoreMIDI soft-reset…");
    softResetCoreMidi();
    await sleep(2000);
    if (!(await isOnConfigurator(page))) {
      await page.goto(CONFIG_URL, {
        waitUntil: "domcontentloaded",
        timeout: 60000,
      });
      await page.waitForTimeout(800);
    }
    ok = await connectDeviceIfNeeded(page, "Reconnect nach MIDIServer-Reset");
  }

  if (!ok) {
    console.log(
      "  ⚠ Auto-reconnect failed — unplug/replug USB, then Connect Device.",
    );
  }
}

const APP_ID_NAME = {
  1: "Control",
  2: "LFO",
  6: "Turing",
  7: "Turing+",
  8: "Euclid",
  9: "Random Triggers",
  10: "Note Fader",
  22: "LFO+",
  23: "FP Grids",
  24: "TB-3PO",
  25: "Automator",
  26: "GenSeq",
  27: "Bernoulli Gate",
  28: "Sift",
  29: "Heat Pump",
  30: "Grooves",
  31: "Golden Gate",
  32: "Super LFO",
  33: "Arp de Lévy",
};

/** Fallback: Active-Apps UI → CH16 + Save (falls Validator-Patch nicht greift). */
async function fixMidiChannel16AfterLoad(page, setup) {
  const need = (setup?.layout || []).filter((lay) =>
    (lay.params || []).some(
      (p) => p?.tag === "MidiChannel" && Number(p.value?.[0]) === 16,
    ),
  );
  if (!need.length) return;

  console.log(
    `[5c/5] CH16 UI follow-up (backup): ${need.length} app(s)…`,
  );

  const appsTab = page.getByRole("tab", { name: /^apps$/i });
  if (await appsTab.count()) {
    await appsTab.first().click().catch(() => {});
    await page.waitForTimeout(600);
  }

  for (const lay of need) {
    const start = (lay.startChannel ?? 0) + 1;
    const name = APP_ID_NAME[lay.appId] || "";
    const result = await page.evaluate(
      ({ name, start }) => {
        const detailsList = [...document.querySelectorAll("form details, details")];
        const details =
          detailsList.find((d) => {
            const t = d.textContent || "";
            if (!name || !t.includes(name)) return false;
            // Channel column: "11" or "9-11"
            return (
              new RegExp(`(?:^|\\D)${start}(?:-\\d+)?(?:\\D|$)`).test(t) ||
              detailsList.filter((x) => (x.textContent || "").includes(name))
                .length === 1
            );
          }) || null;
        if (!details) return { ok: false, reason: `details for ${name}@${start} missing` };
        details.open = true;
        const form = details.closest("form") || details.querySelector("form");
        const inputs = [
          ...details.querySelectorAll(
            'input[name^="param-MidiChannel"], input[type="number"]',
          ),
        ];
        const chInputs = inputs.filter((inp) => {
          if (/param-MidiChannel/i.test(inp.name || "")) return true;
          const block = inp.closest("div")?.textContent || "";
          return /MIDI\s*Channel/i.test(block);
        });
        const targets = chInputs.length ? chInputs : inputs.slice(0, 1);
        if (!targets.length) {
          return { ok: false, reason: "no channel input", nInputs: inputs.length };
        }
        const setter = Object.getOwnPropertyDescriptor(
          HTMLInputElement.prototype,
          "value",
        )?.set;
        for (const inp of targets) {
          setter?.call(inp, "16");
          inp.dispatchEvent(new Event("input", { bubbles: true }));
          inp.dispatchEvent(new Event("change", { bubbles: true }));
        }
        const btn =
          form?.querySelector('button[type="submit"]') ||
          [...(form?.querySelectorAll("button") || [])].find((b) =>
            /^Save$/i.test(b.textContent || ""),
          );
        if (!btn) return { ok: false, reason: "no save", value: targets[0].value };
        btn.click();
        return { ok: true, value: targets[0].value, name };
      },
      { name, start },
    );
    console.log(
      result?.ok
        ? `  → ${name || "app"} fader ${start}: CH16 Save (value “${result.value}”)`
        : `  ⚠ ${name || "app"} fader ${start}: ${result?.reason || result}`,
    );
    await page.waitForTimeout(900);
  }
}

async function loadSetupOnPage(page) {
  const setupJson = JSON.parse(await readFile(SETUP_PATH, "utf8"));
  const expectedTakeover = setupJson?.config?.takeover_mode?.tag;
  const needsCh16 = layoutHasMidiCh16(setupJson);
  if (expectedTakeover) {
    console.log(`Setup takeover: ${expectedTakeover}`);
  }
  if (needsCh16) {
    console.log(
      "[0/5] Setup has MIDI CH16 — no page reload (would kill MIDI); set CH16 via UI after load.",
    );
  }

  const url = page.url();
  const onConfigurator = (() => {
    try {
      return (
        new URL(url).origin === CONFIG_ORIGIN && /#\/configurator/i.test(url)
      );
    } catch {
      return false;
    }
  })();
  if (!onConfigurator) {
    console.log("[1/5] Opening Configurator…");
    await page.goto(CONFIG_URL, { waitUntil: "domcontentloaded", timeout: 60000 });
    await page.waitForTimeout(800);
  } else {
    console.log("[1/5] Reusing existing Configurator tab (no reload — MIDI stays connected).");
  }

  // Beta redirects to the landing page (#/) while disconnected — the
  // Settings tab only exists after Connect Device, so connect FIRST.
  if (!(await connectDeviceIfNeeded(page, "[2/5] Connect Device"))) {
    throw new Error(
      "Timeout: Faderpunk not connected. Click Connect Device in Chrome (allow MIDI permission).",
    );
  }

  console.log("[3/5] Settings tab…");
  const settingsTab = page.getByRole("tab", { name: /settings/i }).or(
    page.locator("text=Settings").first(),
  );
  if (await settingsTab.count()) {
    await settingsTab.first().click();
    await page.waitForTimeout(400);
  } else {
    await page.locator("text=/Settings/i").first().click().catch(() => {});
    await page.waitForTimeout(400);
  }

  console.log("[4/5] Loading setup file…");
  const input = page.locator('input[type="file"]').last();
  await input.waitFor({ state: "attached", timeout: 15000 });
  await input.setInputFiles(SETUP_PATH);
  await page.waitForTimeout(500);

  // First Load opens Recall-Setup modal
  const chooseLoad = page.getByRole("button", { name: /^load$/i }).first();
  await chooseLoad.click({ timeout: 10000 });
  await page.waitForTimeout(800);

  // Modal: ensure app params + global config are recalled (MidiOut lives in app params!)
  console.log("[4b/5] Recall dialog: app parameters + global config…");
  await page.getByText(/Recall Setup/i).first().waitFor({ timeout: 15000 }).catch(() => {});

  await ensureRecallOn(page, /Recall all app parameters/i);
  await ensureRecallOn(page, /Recall global configuration/i);

  console.log("[5/5] Confirm Load (writes params + global config to device)…");
  const modalLoad = page.getByRole("button", { name: /^load$/i }).last();
  await modalLoad.click({ timeout: 10000 });
  // handleSave: setLayout → 1s → params → setGlobalConfig (+ ggf. App-Respawn bei Takeover)
  // Beta health-check can disconnect mid-write — recover below.
  await page.waitForTimeout(3500);

  await recoverConnectionAfterLoad(page);

  // After reconnect, Settings may be gone — ensure we're still useful for takeover/CH16.
  if (await isOnConfigurator(page)) {
    const settingsAgain = page.getByRole("tab", { name: /settings/i });
    if (await settingsAgain.count()) {
      await settingsAgain.first().click().catch(() => {});
      await page.waitForTimeout(300);
    }
  }

  try {
    await ensureTakeoverInSettings(page, expectedTakeover);
  } catch (e) {
    console.log(`  ⚠ Takeover follow-up skipped: ${e.message || e}`);
  }

  if (needsCh16) {
    try {
      await fixMidiChannel16AfterLoad(page, setupJson);
      const dropped =
        !(await isOnConfigurator(page)) ||
        (await page
          .getByRole("button", { name: /connect device/i })
          .isVisible()
          .catch(() => false));
      if (dropped) await recoverConnectionAfterLoad(page);
    } catch (e) {
      console.log(`  ⚠ CH16 UI follow-up failed: ${e.message || e}`);
    }
  }

  console.log("OK — setup loaded. Check Takeover + ports in Settings/App UI.");
}

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
        // Tab auf dem jeweils anderen Configurator (hosted/local): merken,
        // loadSetupOnPage navigiert ihn auf die aufgelöste URL
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
      (found ? ` → Tab ${page.url()}` : " (no Faderpunk tab)"),
  );
  return { browser, page, context, keepOpen: true };
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
    const out = execSync("pgrep -f 'Google Chrome.*faderpunk-scenes/.chrome-profile' || true", {
      encoding: "utf8",
    });
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

/** Only when no profile Chrome is running — never kill a connected WebMIDI session. */
async function launchProfileWithCdp(port) {
  if (profileChromePids().length) {
    throw new Error(
      "Push Chrome is already running, but remote debugging is not responding.\n" +
        "Do not open a second Chrome (WebMIDI is single-owner).\n\n" +
        "Fix: close that Chrome, then push again — or:\n" +
        `  "${CHROME}" --remote-debugging-port=${port} --user-data-dir="${PROFILE}" "${CONFIG_URL}"`,
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

async function main() {
  await mkdir(join(__dirname, "out"), { recursive: true });
  await readFile(SETUP_PATH, "utf8");
  console.log(`Setup: ${SETUP_PATH}`);

  const resolved = await resolveConfigUrl();
  CONFIG_URL = resolved.url;
  CONFIG_ORIGIN = configOrigin(CONFIG_URL);
  console.log(`Configurator: ${CONFIG_URL} [${resolved.source}]`);

  console.log("Looking for CDP (existing Chrome, no kill)…");
  let session = await connectAnyCdp();
  if (!session) {
    for (let i = 0; i < 8 && !session; i++) {
      await sleep(300);
      session = await connectAnyCdp();
    }
  }

  if (!session) {
    console.log("No CDP — starting debug Chrome once…");
    try {
      session = await launchProfileWithCdp(CDP_PORTS[0]);
    } catch (e) {
      throw new Error(String(e.message || e));
    }
  }

  try {
    // WebMIDI-SysEx permission: CDP grants only live per session, so re-grant
    // every time (Chrome had also hard-blocked the prompt in this profile).
    await session.context
      ?.grantPermissions(["midi", "midi-sysex"], { origin: CONFIG_ORIGIN })
      .catch(() => {});
    await loadSetupOnPage(session.page);
    console.log("Chrome stays open (WebMIDI). Next push/pull reuses the same tab.");
  } finally {
    // CDP-Verbindung trennen — sonst hält Playwright den Prozess offen und /api/push antwortet nie.
    // browser.close() bei connectOverCDP schließt Chrome nicht.
    await session?.browser?.close().catch(() => {});
  }
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error(e.message || e);
    process.exit(1);
  });
