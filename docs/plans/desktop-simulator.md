# Faderpunk Desktop Simulator — Status & Roadmap

Status audited and locally rebased 2026-08-08 on branch
`feat/desktop-sim-panel`, onto `origin/main` at `6a9ed793`. The three rewritten
simulator commits are GPG-signed, have not been pushed, and there is no PR.
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

## Phase 1 — PoC (IMPLEMENTED; rebased; partially revalidated)

**The original branch proved that apps run unmodified on macOS and that the
config protocol works over virtual MIDI. The rebased code builds and boots on
Linux; the MIDI/configurator E2E checks still need to be repeated.**

What exists:

- **`fp-core`** (new `no_std` crate): `app.rs`, all 27 apps, `events.rs`,
  `layout.rs`, `state.rs`, `storage.rs`, `macros.rs`, and the portable halves
  of every task — full clock engine (swing/watchdog/gatekeeper), config
  protocol loop, LED effect engine (`LedProcessor`), FRAM channel plumbing.
  Host seams: `StorageBackend` trait (fram.rs), `ConfigSink` trait
  (configure.rs), `platform::init` (RNG + sys-reset hooks), defmt/log `fmt.rs`
  facade, `CoreLocalRawMutex` alias (ThreadModeRawMutex on ARM, CS on host).
- **`faderpunk`** firmware now contains only hardware (pin/SPI/USB/UART/I2C
  drivers + `main.rs`) implementing those seams. The extraction has been
  reconciled with current `main`; firmware clippy and the shared-library gates
  pass. A real-hardware smoke test remains required.
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
- A fresh FRAM now starts the clock running, matching current hardware. The
  rebased headless smoke test observed the expected 48 internal ticks/s at
  120 BPM × 24 PPQN.
- Apps send no MIDI until their "MIDI Out" param is enabled (stock firmware
  behavior); configure via the configurator.
- **Linux/NixOS dependencies are not declared**: the repository devenv lacks
  ALSA development files and the Wayland/Vulkan runtime libraries needed to
  build and launch the panel. Fix the environment before advertising the
  README's plain `cargo build -p fp-sim` command as reproducible on Linux.

## Validation audit — 2026-08-08

Validated after rebasing onto current `main`:

- Upstream app, clock, fader/deadzone, MIDI, calibration, storage, and runtime
  state changes were ported into the extracted architecture.
- The V/Oct configurator flow now uses a portable frequency-measurement backend:
  firmware delegates to the AUX-pin measurement task; the simulator reports a
  calibration error rather than pretending to measure unavailable hardware.
- Both firmware and simulator layout loops service scoped V/Oct app
  eviction/restoration.
- `cargo fmt --all -- --check` passed.
- Firmware clippy for `thumbv8m.main-none-eabihf`, libfp clippy, and fp-core
  clippy passed with warnings denied.
- All 105 libfp unit tests passed.
- `fp-sim` clippy and build passed after supplying ALSA through a temporary Nix
  shell.
- Headless mode booted with both virtual MIDI port pairs and file-backed FRAM;
  a fresh image ran the clock at 48 ticks/s.
- The panel reached the egui/WGPU event loop after supplying Wayland,
  libxkbcommon, Vulkan, and OpenGL runtime libraries.

Not revalidated after the rebase: the configurator handshake, captured MIDI
clock/app output, or real hardware.

Current product boundary: this is an in-repository simulator that requires a
Rust toolchain. There is no child process, source watcher, rebuild-on-save loop,
bundled toolchain, packaged desktop application, or VCV module yet.

## Next steps

**Immediate (before new simulator features):**
1. Review the three signed rewritten commits and the remaining uncommitted
   reconciliation changes, then update the remote branch with force-with-lease.
2. Add Linux host dependencies to the development environment and a host
   build/clippy CI job for `fp-sim`.
3. Repeat the simulator MIDI/configurator checks and smoke-test real hardware
   (boot, apps, configurator, clock, FRAM migration).
4. Add an fp-core/fp-sim section to AGENTS.md. Keep fp-core out of release
   management unless it becomes independently published; decide how fp-sim is
   versioned when packaging produces a distributable artifact.
5. User review of the integrated branch, then PR per repository workflow.

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
