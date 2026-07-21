#!/usr/bin/env node
// Generates src/generated/appLibrary.ts from the Rust source of truth:
// faderpunk/src/apps/mod.rs (registration order) and each app's
// `Config::new(...)` call. The `faderpunk` crate is no_std/embedded-only and
// can't be compiled on the host to introspect at build time, so this reads
// the Rust source text directly instead.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as prettier from "prettier";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(SCRIPT_DIR, "..", "..");
const APPS_DIR = join(REPO_ROOT, "faderpunk", "src", "apps");
const OUT_FILE = join(SCRIPT_DIR, "..", "src", "generated", "appLibrary.ts");

// Mirrors configurator/src/utils/utils.ts's pascalToKebab, reimplemented here
// so this script has no dependency on the TS project.
function pascalToKebab(str) {
  if (!str) return "";
  const camelized = str.replace(/^./, (c) => c.toLowerCase());
  return camelized.replace(/([A-Z])/g, "-$1").toLowerCase();
}

function parseRegisteredApps(modRsSource) {
  const block = modRsSource.match(/register_apps!\(([\s\S]*?)\);/);
  if (!block) {
    throw new Error("Could not find register_apps!(...) block in mod.rs");
  }
  const entries = [];
  const entryRe = /(\d+)\s*=>\s*(\w+)/g;
  let m;
  while ((m = entryRe.exec(block[1]))) {
    entries.push({ id: Number(m[1]), module: m[2] });
  }
  if (entries.length === 0) {
    throw new Error("register_apps!(...) block parsed but no entries found");
  }
  return entries;
}

function parseAppConfig(appRsSource, module) {
  const match = appRsSource.match(
    /Config::new\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*Color::(\w+)\s*,\s*AppIcon::(\w+)/,
  );
  if (!match) {
    throw new Error(`Could not find a Config::new(...) call in ${module}.rs`);
  }
  const [, name, description, color, icon] = match;
  return { name, description, color, icon: pascalToKebab(icon) };
}

const modRs = readFileSync(join(APPS_DIR, "mod.rs"), "utf8");
const registered = parseRegisteredApps(modRs);

const library = registered.map(({ id, module }) => {
  const source = readFileSync(join(APPS_DIR, `${module}.rs`), "utf8");
  return { id, ...parseAppConfig(source, module) };
});

const unformatted = `// GENERATED FILE — do not edit by hand.
// Regenerate with \`./gen-app-library.sh\` from repo root. Source of truth:
// faderpunk/src/apps/mod.rs (order) and each app's \`Config::new(...)\` call.

export interface AppLibraryEntry {
  id: number;
  name: string;
  description: string;
  color: string;
  icon: string;
}

export const APP_LIBRARY: AppLibraryEntry[] = ${JSON.stringify(library, null, 2)};
`;

// No project prettier.config.mjs here on purpose: its only setting is the
// prettier-plugin-tailwindcss plugin, which sorts className strings this
// generated file doesn't have — loading it just adds a plugin-resolution
// dependency for no benefit. Plain defaults already match what eslint
// expects (unquoted keys, trailing commas).
const output = await prettier.format(unformatted, { parser: "typescript" });

mkdirSync(dirname(OUT_FILE), { recursive: true });
writeFileSync(OUT_FILE, output);

console.log(`Wrote ${library.length} apps to ${OUT_FILE}`);
