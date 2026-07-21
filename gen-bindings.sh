#!/usr/bin/env bash
set -euo pipefail
cd gen-bindings
cargo +nightly run --target "$(rustc -vV | sed -n 's|host: ||p')"

# postcard-bindgen emits Latin-1 string (de)serialization (charCodeAt /
# fromCharCode). Postcard on the device uses UTF-8, so non-ASCII AppConfig
# names (e.g. "Arp de Lévy") arrive as mojibake. Re-write the codec to use
# TextEncoder / TextDecoder after every generation.
FP_CONFIG="$(cd .. && pwd)/configurator/node_modules/@atov/fp-config/index.js"
python3 - "$FP_CONFIG" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
t = p.read_text()
old_ser = "serialize_string = (str) => { this.push_n(varint(U32_BYTES, str.length)); const bytes = []; for (const c of str) { bytes.push(c.charCodeAt(0)) } this.push_n(bytes) }"
new_ser = "serialize_string = (str) => { const bytes = Array.from(new TextEncoder().encode(str)); this.push_n(varint(U32_BYTES, bytes.length)); this.push_n(bytes) }"
old_de = "deserialize_string = () => { const str = this.pop_n(Number(this.try_take(U32_BYTES))); return String.fromCharCode(...str) }"
new_de = "deserialize_string = () => { const bytes = this.pop_n(Number(this.try_take(U32_BYTES))); return new TextDecoder(\"utf-8\").decode(Uint8Array.from(bytes)) }"
if old_ser not in t or old_de not in t:
    if new_ser in t and new_de in t:
        print(f"utf-8 string codec already applied: {p}")
        raise SystemExit(0)
    raise SystemExit(f"unexpected string codec in {p}; update gen-bindings.sh")
p.write_text(t.replace(old_ser, new_ser).replace(old_de, new_de))
print(f"patched utf-8 string codec: {p}")
PY

# Drop Vite's prebundled copy so the next `pnpm dev` picks up the patch.
rm -rf "$(cd .. && pwd)/configurator/node_modules/.vite"
echo "cleared configurator/node_modules/.vite"
