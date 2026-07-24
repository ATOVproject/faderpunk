/**
 * Resolve which configurator to automate against:
 *   1. FP_CONFIG_URL env var (explicit override, always wins)
 *   2. FP_CONFIG_PREFER=local|beta|official (editor defaults to beta)
 *   3. Auto: local Vite (:5173) if up, else hosted beta
 */

export const LOCAL_URL = "http://127.0.0.1:5173/#/configurator";
export const BETA_URL = "https://faderpunk.io/beta/#/configurator";
export const OFFICIAL_URL = "https://faderpunk.io/#/configurator";

export const CONFIG_TARGETS = {
  local: { url: LOCAL_URL, label: "Local" },
  beta: { url: BETA_URL, label: "Beta" },
  official: { url: OFFICIAL_URL, label: "Official" },
};

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

/**
 * @param {{ prefer?: "local" | "beta" | "official" | "auto" }} [opts]
 */
export async function resolveConfigUrl(opts = {}) {
  if (process.env.FP_CONFIG_URL) {
    return { url: process.env.FP_CONFIG_URL, source: "env (FP_CONFIG_URL)" };
  }

  const prefer = (
    opts.prefer ||
    process.env.FP_CONFIG_PREFER ||
    "auto"
  ).toLowerCase();

  if (prefer === "local") {
    const up = await localConfiguratorUp();
    return {
      url: LOCAL_URL,
      source: up
        ? "local Vite (forced)"
        : "local Vite (forced — server may be down)",
    };
  }
  if (prefer === "beta") {
    return { url: BETA_URL, source: "hosted beta (forced)" };
  }
  if (prefer === "official") {
    return { url: OFFICIAL_URL, source: "hosted official (forced)" };
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
