#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

if command -v ldd >/dev/null 2>&1; then
    if ldd "$ROOT_DIR/bin/fp-sim" 2>&1 | grep -q "not found"; then
        echo "Error: Missing required system libraries for Faderpunk Simulator on Ubuntu 22.04+." >&2
        echo "Missing libraries reported by ldd:" >&2
        ldd "$ROOT_DIR/bin/fp-sim" 2>&1 | grep "not found" >&2
        echo "Please install the required desktop/audio packages (e.g. libasound2, libfontconfig1, libwayland-client0, libxkbcommon0, etc.)." >&2
        exit 1
    fi
fi

if [ ! -e /dev/snd/seq ]; then
    echo "Error: /dev/snd/seq not found. ALSA sequencer support is required for MIDI." >&2
    echo "Ensure the snd-seq kernel module is available (e.g. run 'sudo modprobe snd-seq' or add your user to the audio group)." >&2
    exit 1
fi

export FP_SIM_CARGO="$ROOT_DIR/toolchain/bin/cargo"
export RUSTC="$ROOT_DIR/toolchain/bin/rustc"
export PATH="$ROOT_DIR/toolchain/bin:$ROOT_DIR/toolchain/lib/rustlib/x86_64-unknown-linux-gnu/bin:$PATH"

export CARGO_HOME="$ROOT_DIR/cache/cargo-home"
export CARGO_TARGET_DIR="$ROOT_DIR/cache/target"
export FP_SIM_CARGO_FROZEN=1
export FP_SIM_FRAM="$ROOT_DIR/cache/fp-sim-fram.bin"
export FP_SIM_PANEL_STATE="$ROOT_DIR/cache/fp-sim-panel.json"

mkdir -p "$ROOT_DIR/cache/cargo-home"

exec "$ROOT_DIR/bin/fp-sim" --project "$ROOT_DIR/workshop-app" "$@"
