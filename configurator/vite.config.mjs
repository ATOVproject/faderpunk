import { readFileSync } from "fs";
import { resolve } from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";

/**
 * Read firmware version from release-please manifest.
 * Uses beta manifest when RELEASE_CHANNEL=beta, otherwise stable.
 */
function getFirmwareVersion() {
  const isBeta = process.env.RELEASE_CHANNEL === "beta";
  const manifestFile = isBeta
    ? ".release-please-manifest.beta.json"
    : ".release-please-manifest.json";

  try {
    const manifestPath = resolve(__dirname, "..", manifestFile);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf-8"));
    return manifest.faderpunk;
  } catch {
    console.warn(`Could not read ${manifestFile}, using fallback version`);
    return "0.0.0"; // Fallback for local dev without manifest
  }
}

// https://vite.dev/config/
export default defineConfig({
  base: process.env.BASE_URL || "/",
  plugins: [react(), tailwindcss()],
  define: {
    __FIRMWARE_LATEST_VERSION__: JSON.stringify(getFirmwareVersion()),
  },
});
