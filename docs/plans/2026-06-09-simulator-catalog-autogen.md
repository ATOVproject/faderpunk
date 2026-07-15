# Auto-Generate the Simulator App Catalog from Firmware

**Status:** Planning / not started. Captured 2026-06-09.

**Goal:** Replace the hand-maintained `configurator/src/demo/catalog.ts` (~392 lines of
manually mirrored app metadata) with a catalog that is generated from the firmware's own
app definitions, so it cannot drift from the real device.

**Context:** The catalog was introduced with simulator mode (branch `feat/simulator-mode`).
It carries a `TODO: Auto-regenerate this file on every release`. The original note said
"via gen-bindings", but the better hook is **the host build step that already runs when the
firmware/bindings are built** (see below).

---

## Background: where the data actually lives

The catalog is **exactly** the data the firmware already sends over USB. When a real device
is connected, `getAllApps` (`configurator/src/utils/config.ts:39`) sends a `GetAllApps`
request and parses a batch of `AppConfig` messages into the `App` objects the UI consumes.

Firmware side (`libfp/src/lib.rs`):

```rust
#[derive(Clone, Serialize, PostcardBindings)]
pub enum ConfigMsgOut<'a> {
    // ...
    AppConfig(u8, usize, ConfigMeta<'a>),   // (appId, channels, meta)
    // ...
}

// ConfigMeta = (paramCount, name, description, color, icon, &[Param])
pub fn get_meta(&self) -> ConfigMeta<'_> {
    (N, self.name, self.description, self.color, self.icon, &self.params)
}
```

Each app declares this as a compile-time `const CONFIG`, e.g. `faderpunk/src/apps/control.rs:22`:

```rust
pub static CONFIG: Config<PARAMS> = Config::new(
    "Control", "Simple MIDI/CV controller", Color::Violet, AppIcon::Fader,
)
.add_param(Param::Curve { /* ... */ })
// ...
.add_param(Param::MidiOut);
```

And `register_apps!` (`faderpunk/src/macros.rs`) exposes:

```rust
pub fn get_config(app_id: u8) -> Option<(u8, usize, ConfigMeta<'static>)> {
    match app_id {
        $( $id => Some((app_id, $app_mod::CHANNELS, $app_mod::CONFIG.get_meta())), )*
        _ => None,
    }
}
```

The app registry itself is `faderpunk/src/apps/mod.rs` (`register_apps! { 1 => control, ... 23 => fp_grids }`).

**Bottom line:** `demo/catalog.ts` is a hand-copy of `(id, CHANNELS, ConfigMeta)` for every
registered app. We want to dump that programmatically instead of maintaining it by hand.

---

## The one real blocker

The `CONFIG` statics live **inside the embedded app modules** (`faderpunk/src/apps/*.rs`).
Those modules pull in embassy, the RP2350 HAL, defmt, etc. and only compile for the
`thumbv8m` target. A host tool cannot import them as-is.

However, the **data types** they use (`Config`, `Param`, `Color`, `AppIcon`, `Range`,
`Curve`, …) all live in `libfp`, which **does** compile for the host. Proof: the existing
`gen-bindings` tool depends on `libfp` and runs `cargo run` on the host
(`gen-bindings.sh`), and it's invoked in CI before the configurator build
(`.github/workflows/ci.yml:55`, `beta.yml:172`, `release.yml:179`):

```sh
# gen-bindings.sh
cd gen-bindings
cargo +nightly run --target $(rustc -vV | sed -n 's|host: ||p')
```

So the task reduces to: **make the app metadata reachable from a host build, then dump it
from the same host step that already runs at build time.**

---

## Recommended approach

### Step 1 — Decouple app metadata from the embedded runtime

Move each app's `CONFIG` / `CHANNELS` / id out of the embedded module into a
**host-compilable shared location**. Two viable homes:

- **(Recommended) New workspace crate `app-catalog`** — holds only the const
  `CONFIG`/`CHANNELS`/id data, depends solely on `libfp` data types. Both `faderpunk`
  (embedded) and the new host generator depend on it. Keeps `libfp`'s public surface
  unchanged.
- **`libfp::catalog` module** — fewer new files, but grows `libfp`'s public API with
  app-specific data.

The firmware apps then reference the shared `CONFIG`/`CHANNELS` instead of defining them
locally. This makes the shared crate the single source of truth so firmware and generator
**cannot drift**.

> Note on const generics: `Config<const N>` has a per-app size, so you can't put the configs
> in one homogeneous array. Mirror the existing pattern — a registry macro / match like
> `register_apps!` that lists `id => module` and produces `Vec<(u8, usize, ConfigMeta)>` via
> each `CONFIG.get_meta()`. The statics are `'static`, so `get_meta()` yields
> `ConfigMeta<'static>`.

This refactor (touching ~23 apps) is the bulk of the effort. Steps 2–3 are small once the
data is host-reachable.

### Step 2 — Emit the catalog from the host generator

Extend `gen-bindings` (or add a sibling host bin `gen-catalog`) to iterate every registered
app, build the same `AppConfig(id, channels, ConfigMeta)` values the firmware sends, and
serialize them. Two output options:

- **(Recommended) Postcard blob.** Serialize the `AppConfig` batch with `postcard` into a
  small committed blob (e.g. `configurator/src/demo/catalog.bin` or base64 in a `.ts`). The
  configurator decodes it with the **already-generated postcard-bindgen deserializer** it
  uses for the USB path. Benefits: byte-identical to real-device data (zero shape-mismatch
  risk), and `catalog.ts` collapses to a ~10-line loader. Downside: opaque in diffs.
- **JSON / TS data.** Emit `catalog.json`. Reviewable diffs, but Rust enums don't serialize
  to the TS `{ tag, value }` shape by default — needs a custom `Serialize` impl or
  hand-mapping in the generator to match the postcard-bindgen JS representation.

### Step 3 — Wire into the build + prevent drift

- Call the generator from `gen-bindings.sh` (already runs in ci/beta/release before
  `pnpm build`), so the catalog regenerates whenever bindings are generated.
- Add a CI step that regenerates and runs `git diff --exit-code` on the output, so any
  firmware change that alters an app config fails CI until the catalog is regenerated.

---

## Configurator-side changes

- `configurator/src/demo/catalog.ts`: replace the 392 lines of hand-written `App` data with
  a loader that decodes the generated blob/JSON into `AllApps` (the existing
  `Map<number, App>`), reusing the same decode path as `getAllApps` where possible.
- Remove the `TODO: Auto-regenerate this file on every release` once done.
- Keep `DEMO_APPS` as the exported `AllApps` so `store.ts` (`connectSimulator`,
  `loadPersistedSimulatorState`) needs no changes.

---

## Open decisions to settle before implementing

1. **Output format:** postcard blob (recommended) vs JSON/TS.
2. **Metadata home:** new `app-catalog` crate (recommended) vs `libfp::catalog` module.
3. **Rollout:** prototype one app end-to-end first, or do the full ~23-app refactor in one go.

## Risks / notes

- The decoupling refactor touches every app module; do it mechanically and rely on the
  firmware build to catch mistakes (it already builds in CI).
- If using the postcard-blob route, confirm the generated JS deserializer for the
  `AppConfig` batch is importable standalone (the USB path already imports `deserialize*`
  from `@atov/fp-config`).
- Param order matters: the configurator maps stored params to app params **by index**
  (`recoverLayout` → `params[idx]` in `config.ts`). Generating from the firmware guarantees
  the order matches, which is the main reason this is worth doing.

## Key references

- Catalog: `configurator/src/demo/catalog.ts`
- USB fetch path mirrored by the catalog: `configurator/src/utils/config.ts:39` (`getAllApps`)
- App registry: `faderpunk/src/apps/mod.rs`, `faderpunk/src/macros.rs`
- App config example: `faderpunk/src/apps/control.rs:22`
- Types: `libfp/src/lib.rs` (`Config`, `ConfigMeta`, `ConfigMsgOut`, `Param`)
- Existing host generator + hook: `gen-bindings/src/main.rs`, `gen-bindings.sh`,
  `.github/workflows/ci.yml:55`
