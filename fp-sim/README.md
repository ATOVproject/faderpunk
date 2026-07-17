# fp-sim — Faderpunk desktop simulator (headless PoC)

Runs the unmodified `fp-core` app/clock/config stack on the embassy std
executor with virtual hardware:

- **MIDI**: two virtual port pairs (CoreMIDI on macOS, ALSA on Linux),
  mirroring the hardware's two USB-MIDI cables:
  - **"Faderpunk Sim"** — performance MIDI (clock, transport, notes, CCs).
    Use this one in your DAW.
  - **"Faderpunk Sim Config"** — the configurator SysEx protocol. The web
    configurator's port discovery (`/faderpunk/i` name match + GetVersion
    probe) finds it like a real device.
- **Transport**: like a fresh device, the clock starts *stopped* — press
  **Enter** in the simulator's terminal to start/stop it (the hardware's
  Shift+Scene). The running state persists to the FRAM image across
  restarts. With the clock running, MIDI clock + transport stream to the
  performance port (default global config sends both).
- **FRAM**: file-backed image, `fp-sim-fram.bin` in the working directory
  (override with `FP_SIM_FRAM=/path/to/image`). Delete the file for a
  factory-fresh device.
- **MAX11300 / LEDs / faders / buttons**: headless stand-ins for now. CV
  outputs land in the shared `MAX_VALUES_DAC` atomics; channel 0 is printed
  once per 250 ms. A panel UI replaces these in the next phase.

## Run

```bash
cargo build -p fp-sim          # note: -p required, fp-sim is not a default member
RUST_LOG=info ./target/debug/fp-sim

# Force the LFO app onto channel 0 (useful without a UI to change the layout):
FP_SIM_LFO=1 RUST_LOG=info ./target/debug/fp-sim
```

Then open the configurator (`pnpm -C configurator dev`) in Chromium and
connect — the simulator shows up as "Faderpunk Sim". Layout and parameter
changes persist to the FRAM image like on hardware.

To get app MIDI (e.g. the LFO as a CC stream) into a DAW: enable the app's
"MIDI Out" parameter (USB target) in the configurator, pick channel/CC, and
start the clock transport. Then map the CC in the DAW from the
"Faderpunk Sim" port.

**Troubleshooting**: if a port is dead, check for a stale simulator instance
(`pgrep -fl fp-sim`) — an old process keeps its virtual ports alive and your
DAW may have connected to those.

## Environment variables

| Variable      | Effect                                              |
| ------------- | --------------------------------------------------- |
| `RUST_LOG`    | Log level (`info` default, `debug` for MAX/MIDI)    |
| `FP_SIM_FRAM` | Path of the FRAM image (default `fp-sim-fram.bin`)  |
| `FP_SIM_LFO`  | If set, forces the LFO app (id 2) onto channel 0    |

## Known PoC limitations

- No fader/button input yet (needs the panel UI phase); apps that wait on
  input events simply idle. The only control is the Enter transport toggle.
- Only the "USB" MIDI target (index 0) is bridged; DIN 1/2 targets have no
  physical counterpart.
- Windows: `midir` cannot create virtual ports there; use loopMIDI (untested).
- Firmware version reported to the configurator is hardcoded in
  `src/main.rs` (`FIRMWARE_VERSION`).
