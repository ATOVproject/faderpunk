#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

if ! /usr/bin/xcrun --find clang >/dev/null 2>&1; then
    echo "Error: Apple Command Line Tools (clang/linker) are required." >&2
    echo "Please install them by running: xcode-select --install" >&2
    exit 1
fi

export FP_SIM_CARGO="$ROOT_DIR/toolchain/bin/cargo"
export RUSTC="$ROOT_DIR/toolchain/bin/rustc"
export PATH="$ROOT_DIR/toolchain/bin:$ROOT_DIR/toolchain/lib/rustlib/aarch64-apple-darwin/bin:$PATH"

export CARGO_HOME="$ROOT_DIR/cache/cargo-home"
export CARGO_TARGET_DIR="$ROOT_DIR/cache/target"
export FP_SIM_CARGO_FROZEN=1
export FP_SIM_FRAM="$ROOT_DIR/cache/fp-sim-fram.bin"
export FP_SIM_PANEL_STATE="$ROOT_DIR/cache/fp-sim-panel.json"

mkdir -p "$ROOT_DIR/cache/cargo-home"

exec "$ROOT_DIR/bin/fp-sim" --project "$ROOT_DIR/workshop-app" "$@"
