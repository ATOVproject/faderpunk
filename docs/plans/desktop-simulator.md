# Faderpunk Desktop Simulator — Status & Roadmap

Status updated 2026-08-08 on branch `feat/desktop-sim-panel`, rebased onto
`origin/main` at `6a9ed793`. Phase 2 is implemented in the current uncommitted
worktree; the four pre-Phase-2 commits are GPG-signed and have not been pushed.
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

Validated after the current-main rebase:

- Upstream app, clock, fader/deadzone, MIDI, calibration, storage, and runtime
  state changes were ported into the extracted architecture.
- The portable V/Oct flow delegates frequency measurement to hardware and
  reports unavailable measurement in the simulator.
- Firmware and simulator layout loops service scoped app eviction/restoration.
- Firmware, `libfp`, `fp-core`, and the original simulator passed their
  applicable format, clippy, unit-test, build, and headless boot gates.
- Fresh simulator FRAM ran the internal clock at 48 ticks/s.
- The panel reached the egui/WGPU event loop with the required Linux runtime
  libraries.

Validated for Phase 2:

- `fp-sim-protocol` framing tests cover sequential messages, clean EOF,
  truncated prefixes, and the allocation limit.
- `fp-sim`, `fp-sim-core`, and `fp-sim-protocol` build and pass clippy with
  warnings denied.
- The host launches the child, receives `Ready`/state frames, and replays
  physical fader/button/ADC state.
- Touching an in-tree core source rebuilt and booted exactly one replacement
  child. ALSA client/port IDs before and after were identical.
- The standalone `fp-sim-app-example` contract builds and runs app id 128.
  Touching its source rebuilt only the external binary and booted exactly one
  replacement child.
- A deliberate external-app compiler error was rendered and surfaced in status;
  the original child PID stayed alive with no extra boot. Restoring valid code
  produced exactly one successful replacement boot.
- Parent-state tests cover fader persistence across launches and complete
  physical-input replay after child replacement.
- `devenv.nix` now supplies ALSA, Wayland, libxkbcommon, Vulkan, and OpenGL
  host dependencies on Linux. The panel booted through WGPU, received child
  `Ready` v1.12.0, and replayed panel state using that environment.
- CI now lints/tests the three simulator crates and checks the standalone
  external app project.
- Full firmware, `libfp`, `fp-core`, simulator, and external-example format,
  clippy, and applicable unit-test gates passed; `actionlint` accepted the CI
  workflow.

Still not revalidated after the rebase: the full Chromium configurator
handshake, captured performance MIDI output, or real hardware.

Current product boundary: the complete in-repository developer loop exists,
but it still requires a host Rust toolchain and platform libraries. No bundled
toolchain, packaged desktop application, or VCV module exists yet.

## Next steps

**Immediate integration and validation:**

1. Review the Phase 2 worktree, then commit and push only after explicit user
   approval.
2. Repeat the configurator handshake and performance MIDI capture against the
   parent/child host.
3. Smoke-test real hardware after the `fp-core` registry/config enumeration
   change.
4. Decide simulator version/release ownership before packaging; keep `fp-core`
   and protocol internals out of Knope unless independently published.

**Phase 2 — Simulator app (IMPLEMENTED in worktree):**

- Stable `fp-sim` parent owns egui, physical panel state, persisted faders, and
  both virtual MIDI port pairs.
- Rebuildable `fp-sim-core` child owns Embassy, portable firmware logic, virtual
  MAX/LED/input tasks, and file-backed FRAM.
- `fp-sim-protocol` carries length-prefixed postcard frames on child
  stdin/stdout; logs stay on stderr.
- Successful source builds replace the child; failed builds leave the previous
  child running and surface the error in the UI.
- Child startup, EOF, shutdown, unexpected exit, and restart paths are handled.
- Parent replays faders, held buttons, and ADC values after every `Ready`.
- The transport bar displays child version, build status, BPM, swing,
  transport, and current scene.
- `fp_core::register_external_app!`, runtime descriptors, and
  `fp-sim-app-example` define the standalone app-project contract.
- `--project PATH` watches the user's crate and lets Cargo recompile the app
  binary while dependencies remain cached.
- `fp-sim/README.md` and `AGENTS.md` document the run path and architecture.

**Phase 3 — Packaging (Arduino model):**
- Bundle pinned toolchain + prebuilt target dir into the app; private
  RUSTUP_HOME/CARGO_HOME; investigate `rust-lld`/self-contained linking on
  macOS (else require Xcode CLT initially); signing/notarization.

**Phase 4 — VCV Rack module:**
- C++ shim + fp-core/sim-core as staticlib; CV via the `MAX_VALUES_*` atomics
  in `process()`; single instance per patch (global statics) for v1;
  macOS/Linux first.

**Deferred/optional:** hosted browser sim (fp-core is target-clean), WASM
hot-reload plugins, and Windows virtual-MIDI support (loopMIDI).

## Phase 2 decisions (implemented 2026-08-08)

- **Process boundary**: `fp-sim` is the stable UI/MIDI/watch parent;
  `fp-sim-core` is the replaceable firmware core child.
- **IPC**: length-prefixed postcard frames over stdin/stdout; child logs on
  stderr. Parent owns physical panel and MIDI state; child owns FRAM/runtime
  state.
- **Rebuild policy**: build before stopping the current child. Swap only after
  a successful build, then replay all physical inputs after `Ready`.
- **External app boundary**: one binary calls `fp_sim_core::run` with static
  descriptors produced by `register_external_app!`; IDs 128–255 are the
  recommended user range.
- **Fader layers**: `libfp::latch::AnalogLatch` stays shared; the child panel
  task retains the hardware-faithful sweep over parent-supplied positions.
