# fp-sim — Faderpunk desktop simulator

`fp-sim` is the long-lived desktop host: it owns the panel window, physical
fader/button/ADC state, and both virtual MIDI port pairs. It builds and launches
`fp-sim-core` as a replaceable child process. The child runs the unmodified
`fp-core` app, clock, layout, storage, and config stack on Embassy's std
executor with virtual hardware.

- **Rebuild on save**: changes under `fp-sim-core/src`, `fp-core/src`, or
  `libfp/src` trigger a Cargo build. A successful build swaps the child without
  closing the panel or MIDI ports. A failed build leaves the previous child
  running and displays the build failure in the transport bar.
- **Panel window**: 16 channel strips with LEDs, fader, button, and CV jack,
  plus SCENE/SHIFT, the three aux jacks, transport, current scene, and build
  status. CV inputs are drag-editable. The bar beside each fader shows the
  latched app-visible value and turns amber while takeover is pending.
  - Hold **Shift** = SHIFT, **Ctrl/Cmd** = SCENE, **Space** = transport.
  - Hold SCENE while moving faders to edit global settings with hardware-faithful
    takeover behavior.
- **MIDI**: parent-owned virtual port pairs remain stable across child rebuilds:
  - **"Faderpunk Sim"** — performance MIDI for DAWs.
  - **"Faderpunk Sim Config"** — configurator SysEx; device discovery sees it
    like real hardware.
- **Persistent state**:
  - FRAM: `fp-sim-fram.bin` (override with `FP_SIM_FRAM`).
  - Physical panel faders: `fp-sim-panel.bin` (override with
    `FP_SIM_PANEL_STATE`). The parent also retains buttons and ADC inputs while
    replacing a child.
- **Firmware version**: mirrored from `faderpunk/Cargo.toml` by the child build.

## Run

Linux requires ALSA development libraries; the panel also needs the normal
Wayland/X11 and graphics runtime libraries. From the repository root:

```bash
# Build the stable host once. The host builds/rebuilds its core child itself.
cargo build -p fp-sim
RUST_LOG=info ./target/debug/fp-sim

# No panel; Enter toggles transport and q+Enter quits.
./target/debug/fp-sim --headless

# Force any registered app onto channel 0.
FP_SIM_APP_ID=2 ./target/debug/fp-sim
```

Then run the configurator (`pnpm -C configurator dev`) in Chromium and connect
to "Faderpunk Sim". Layout and parameter changes persist in the FRAM image.
For app MIDI, enable its USB MIDI output in the configurator and use the
"Faderpunk Sim" performance port in the DAW.

### External app project

Pass a standalone Rust crate to `--project`; the host watches its `src`
directory and `Cargo.toml`, invokes Cargo, and swaps in its one binary:

```bash
./target/debug/fp-sim --project fp-sim-app-example
FP_SIM_APP_ID=128 ./target/debug/fp-sim --project fp-sim-app-example
```

The crate contract is:

1. Produce exactly one binary and depend on `fp-core`, `fp-sim-core`, `libfp`,
   `embassy-executor`, `embassy-futures`, and `embassy-sync`.
2. Implement the ordinary firmware app contract: `CHANNELS`, `CONFIG`, and an
   Embassy `wrapper(App<CHANNELS>, exit_signal)` task.
3. Register each app with `fp_core::register_external_app!`; use IDs outside
   the built-in range (128–255 is recommended).
4. Put the descriptors in a static array and call `fp_sim_core::run(&APPS)` from
   `main`.

`fp-sim-app-example` is a complete minimal project. Cargo recompiles the app
crate on save while cached dependencies remain built.

**Troubleshooting**: a stale host keeps its MIDI ports alive. Check with
`pgrep -fl fp-sim` before starting another instance.

## Environment variables

| Variable             | Effect                                                  |
| -------------------- | ------------------------------------------------------- |
| `RUST_LOG`           | Parent and child log level (`info` by default)          |
| `FP_SIM_FRAM`        | FRAM image path (default `fp-sim-fram.bin`)             |
| `FP_SIM_PANEL_STATE` | Panel fader image path (default `fp-sim-panel.bin`)     |
| `FP_SIM_APP_ID`      | Force a registered built-in or external app on channel 0 |
| `FP_SIM_LFO`         | Compatibility alias that forces built-in app id 2       |
| `FP_SIM_CARGO`       | Cargo executable used by the rebuild manager            |
| `FP_SIM_HEADLESS`    | Run the host without a panel window                      |
| `FP_SIM_MONITOR`     | Log virtual channel 0 fader/DAC/tick state in the child  |

## Architecture notes

The parent and child communicate over length-prefixed postcard frames on the
child's stdin/stdout; child logs go to stderr. Messages carry physical inputs,
performance/config MIDI, rendered hardware state, transport state, version,
and scene. The parent replays its complete physical input snapshot after every
child `Ready` frame, so a rebuild does not move a fader, release a held button,
or reset a CV input.

The child owns `panel.rs`, which reproduces firmware long-press,
scene-hold/load/save, SHIFT+SCENE, and `AnalogLatch` behavior. `hw.rs` consumes
the same MAX11300 and LED channels as hardware tasks. Apps therefore run
against the same `fp-core` APIs in firmware, the built-in child, and external
app projects.

Virtual MIDI is deliberately parent-owned. Rebuilding an app never changes the
CoreMIDI/ALSA endpoint identity seen by a configurator or DAW.

## Known limitations

- Only USB MIDI target 0 is bridged; DIN 1/2 have no desktop counterpart.
- Windows virtual ports need an external tool such as loopMIDI and remain
  untested.
- Calibration-related behavior is not validated in the simulator.
- Packaging a pinned Rust toolchain/dependency cache is Phase 3; this checkout
  still requires a working host Rust/Cargo environment.
