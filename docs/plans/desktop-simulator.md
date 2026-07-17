# Faderpunk Desktop Simulator — Status & Roadmap

Working state as of 2026-07-17, branch `feat/desktop-sim-panel`
(stacked on `feat/desktop-sim-poc`, which holds the committed phase 1).
Original feasibility assessment: `~/.claude-cli/plans/i-want-to-gauge-eager-bee.md`.

## The grand plan

Reuse the firmware's app code unchanged on PC/Mac as (1) a simulator with a
panel UI, (2) a MIDI control surface, (3) eventually a VCV Rack module.
Primary audience: **people writing Faderpunk apps**, who need a fast local
code→feedback loop without installing a Rust toolchain themselves.

Decisions settled during planning:

- **Native-first** (not browser-first): real virtual MIDI ports beat zero
  install; a WASM/browser sim stays a possible later add-on.
- **Developer UX end state = the Arduino model**: ship the sim with a bundled
  pinned rustc + precompiled dependency cache; on save, only the user's app
  crate recompiles (~2–3s) into a headless core child process while the UI
  parent keeps all panel state. No CI in the developer loop.
- **UI: prototype in egui, revisit Makepad** for the polished product
  (GPU/shader styling, live design DSL, same-code WASM build; Ironfish synth
  demo is precedent). The parent/child IPC split makes the swap cheap.

## Phase 1 — PoC (DONE, verified)

**Proved: apps run unmodified on macOS, and the config protocol works over
virtual MIDI.**

What exists:

- **`fp-core`** (new `no_std` crate): `app.rs`, all 27 apps, `events.rs`,
  `layout.rs`, `state.rs`, `storage.rs`, `macros.rs`, and the portable halves
  of every task — full clock engine (swing/watchdog/gatekeeper), config
  protocol loop, LED effect engine (`LedProcessor`), FRAM channel plumbing.
  Host seams: `StorageBackend` trait (fram.rs), `ConfigSink` trait
  (configure.rs), `platform::init` (RNG + sys-reset hooks), defmt/log `fmt.rs`
  facade, `CoreLocalRawMutex` alias (ThreadModeRawMutex on ARM, CS on host).
- **`faderpunk`** firmware now contains only hardware (pin/SPI/USB/UART/I2C
  drivers + `main.rs`) implementing those seams. Behavior unchanged; all CI
  gates green; UF2 builds.
- **`fp-sim`** (headless): embassy `arch-std` executor; two virtual MIDI port
  pairs mirroring the USB cables — "Faderpunk Sim" (performance) and
  "Faderpunk Sim Config" (configurator); file-backed FRAM
  (`fp-sim-fram.bin` / `FP_SIM_FRAM`); logging stand-ins for MAX11300/LEDs;
  Enter = transport start/stop (persisted); `FP_SIM_LFO=1` forces LFO on ch 0.
  See `fp-sim/README.md`.

Verified end-to-end: LFO generates its sine into the virtual DAC; clock ticks
at 120 BPM×24 PPQN; MIDI clock streams 48 msgs/s on the perf port once the
transport is started; a probe client got Ping→Pong, GetVersion→1.11.0 and all
27 apps over the config port (the exact configurator handshake).

Gotchas already learned (don't relearn):

- **Workspace feature leak**: fp-sim's std features poison the thumb build if
  co-selected → `default-members` excludes fp-sim; build with
  `cargo build -p fp-sim`.
- **Zombie sims hold their virtual ports**; DAWs latch onto the dead one.
  `pkill -f fp-sim` before restarting.
- Fresh FRAM = clock stopped (like hardware) — nothing streams until the
  transport starts.
- Apps send no MIDI until their "MIDI Out" param is enabled (stock firmware
  behavior); configure via the configurator.

## Next steps

**Immediate (before new work):**
1. User review of the branch → commit (conventional message) → PR per repo
   workflow. Firmware smoke test on real hardware (boot, apps, configurator,
   FRAM migration) since main.rs/tasks were restructured.
2. Decide whether AGENTS.md gets an fp-core/fp-sim section (crate map, the
   default-members rule, sim build/run commands) and whether knope/release
   config should know about the new crates.

**Phase 2 — Simulator app (panel UI + dev loop):**

*Done (this branch):*
- egui panel (`fp-sim/src/ui.rs`, eframe 0.35): 16 channel strips (top/bottom
  LEDs, fader, lit button, jack cell), SCENE/SHIFT, aux jacks, transport bar.
  Keyboard: Shift=SHIFT, Ctrl/Cmd=SCENE, Space=transport. `--headless` /
  `FP_SIM_HEADLESS` keeps the phase-1 mode.
- `fp-sim/src/panel.rs`: firmware-faithful input semantics — button
  long-press/scene-hold/SHIFT+SCENE from `tasks/buttons.rs` (minus GPIO
  debounce) and the `AnalogLatch` fader layers from `read_fader` (global
  settings via SCENE-hold work, incl. takeover). UI↔core boundary is only
  thread-safe statics (`SIM_FADER_POS`, `set_button`, `LED_FRAME`, MAX
  atomics) — deliberately narrow so the process split later is a transport
  swap. Executor runs on a background thread; UI owns main (macOS rule).
- Virtual MAX now tracks port modes + DAC/ADC ranges + `Gpo*` gate states;
  jack cells render per mode, ADC inputs are drag-editable in the UI.
- LED frame published with brightness applied (`hw::LED_FRAME`).
- Firmware version mirrored from `faderpunk/Cargo.toml` via `fp-sim/build.rs`
  (prerelease suffixes tolerated).
- fp-core: new `CLOCK_RUNNING` atomic mirrored by the clock gatekeeper (UI
  transport indicator; useful for VCV later).

*Remaining in phase 2:*
- Parent/child process split: UI parent owns state + file watching, headless
  core as child; IPC decided: length-prefixed postcard frames over
  stdin/stdout (logs on stderr); `cargo watch`-style rebuild of the child on
  app-source change. Split fp-sim into `fp-sim` (UI parent) + `fp-sim-core`
  (child) when this lands.
- Nice-to-haves surfaced while building: persist panel fader positions
  across restarts (parent-side state), current-scene indicator in the
  transport bar.

**Phase 3 — Packaging (Arduino model):**
- Bundle pinned toolchain + prebuilt target dir into the app; private
  RUSTUP_HOME/CARGO_HOME; investigate `rust-lld`/self-contained linking on
  macOS (else require Xcode CLT initially); signing/notarization.

**Phase 4 — VCV Rack module:**
- C++ shim + fp-core/sim-core as staticlib; CV via the `MAX_VALUES_*` atomics
  in `process()`; single instance per patch (global statics) for v1;
  macOS/Linux first.

**Deferred/optional:** hosted browser sim (wasm — fp-core is already
target-clean), wasm hot-reload app plugins, out-of-tree app SDK, Windows
(loopMIDI; no user-space virtual ports).

## Phase 2 decisions (settled 2026-07-17)

- **In-process first**: the panel UI ships inside fp-sim; the parent/child
  split happens later in phase 2 when the rebuild-on-save loop is built.
- **IPC when split**: length-prefixed postcard frames over the child's
  stdin/stdout, logs on stderr. State ownership: fader/panel state in the
  parent, FRAM in the child.
- **Crate split** (`fp-sim` UI parent + `fp-sim-core` headless child)
  deferred to the split itself.
- **Fader layers**: solved without extracting `read_fader` —
  `libfp::latch::AnalogLatch` is already portable; `fp-sim/src/panel.rs`
  re-implements the thin sweep loop over UI slider positions.
