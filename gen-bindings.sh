#!/usr/bin/env bash
cd gen-bindings
cargo +nightly run --target $(rustc -vV | sed -n 's|host: ||p')
