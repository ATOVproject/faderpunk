#!/usr/bin/env node
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";

const profile = join(homedir(), ".faderpunk-diagnostics-chrome");
const url = process.argv[2] ?? "http://127.0.0.1:3850/";
const chrome =
  process.platform === "darwin"
    ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    : "google-chrome";

spawn(
  chrome,
  [`--user-data-dir=${profile}`, "--enable-features=WebMIDIPermission", url],
  { detached: true, stdio: "ignore" },
).unref();
