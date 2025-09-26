#!/usr/bin/env bash
cd faderpunk
cargo build --release
cd ..
cp target/thumbv8m.main-none-eabihf/release/faderpunk target/thumbv8m.main-none-eabihf/release/faderpunk.elf
picotool uf2 convert target/thumbv8m.main-none-eabihf/release/faderpunk.elf target/thumbv8m.main-none-eabihf/release/faderpunk.uf2
