/**
 * Resolve which configurator to automate against:
 *   1. FP_CONFIG_URL env var (explicit override, always wins)
 *   2. Local Vite dev configurator (http://127.0.0.1:5173) if it responds
 *   3. Hosted beta configurator (https://faderpunk.io/beta) as fallback
 *
 * Local is preferred because it is built from the same branch as the flashed
 * firmware (protocol always in sync, CH16 fix in source, manuals for custom
 * apps). The hosted beta works with the same SysEx protocol but may drift
 * when upstream redeploys.
 */

const LOCAL_URL = "http://127.0.0.1:5173/#/configurator";
const BETA_URL = "https://faderpunk.io/beta/#/configurator";

async function localConfiguratorUp(timeoutMs = 1200) {
  try {
    const res = await fetch("http://127.0.0.1:5173/", {
      signal: AbortSignal.timeout(timeoutMs),
    });
    return res.ok;
  } catch {
    return false;
  }
}

export async function resolveConfigUrl() {
  if (process.env.FP_CONFIG_URL) {
    return { url: process.env.FP_CONFIG_URL, source: "env (FP_CONFIG_URL)" };
  }
  if (await localConfiguratorUp()) {
    return { url: LOCAL_URL, source: "local Vite dev server" };
  }
  return { url: BETA_URL, source: "hosted beta (local :5173 not running)" };
}

export function configOrigin(url) {
  try {
    return new URL(url).origin;
  } catch {
    return "http://127.0.0.1:5173";
  }
}
