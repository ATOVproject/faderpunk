#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"

cargo +stable build --bin faderpunk --release --target thumbv8m.main-none-eabihf
cp target/thumbv8m.main-none-eabihf/release/faderpunk target/thumbv8m.main-none-eabihf/release/faderpunk.elf
picotool uf2 convert target/thumbv8m.main-none-eabihf/release/faderpunk.elf target/thumbv8m.main-none-eabihf/release/faderpunk.uf2
