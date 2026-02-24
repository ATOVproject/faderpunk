import { readFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Read firmware version from faderpunk/Cargo.toml (the source of truth for knope).
 * Falls back to 0.0.0 for local dev if the file can't be read.
 */
export function getFirmwareVersion() {
  try {
    const cargoPath = resolve(__dirname, "..", "faderpunk", "Cargo.toml");
    const cargo = readFileSync(cargoPath, "utf-8");
    const match = cargo.match(/^version\s*=\s*"(.+?)"/m);
    if (match) return match[1];
    throw new Error("version field not found in Cargo.toml");
  } catch (e) {
    console.warn(
      `Could not read firmware version: ${e.message}, using fallback`,
    );
    return "0.0.0";
  }
}
