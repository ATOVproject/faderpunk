# fp-sim — Faderpunk desktop simulator

Runs the unmodified `fp-core` app/clock/config stack on the embassy std
executor with virtual hardware, plus a panel UI:

- **Panel window** (default): 16 channel strips — top/bottom LEDs, fader,
  button (lit by its LED), and the channel's CV jack — plus SCENE/SHIFT
  buttons, the three aux jacks, and transport control. Jacks render by their
  configured mode: CV outputs show voltage, gate outputs show a lamp, CV
  *inputs* are drag-editable to feed values into apps. The thin bar next to
  each fader shows the latched value apps actually see (amber while takeover
  is pending, i.e. the physical fader hasn't picked up the target yet).
  - Hold **Shift** (keyboard) = SHIFT button, hold **Ctrl** (or Cmd) = SCENE
    button, **Space** = transport start/stop.
  - Fader "layers" work like hardware: hold SCENE and move faders to set
    global settings (BPM on fader 16, swing on 15, LED brightness on 1, …),
    with the configured takeover mode.
- **MIDI**: two virtual port pairs (CoreMIDI on macOS, ALSA on Linux),
  mirroring the hardware's two USB-MIDI cables:
  - **"Faderpunk Sim"** — performance MIDI (clock, transport, notes, CCs).
    Use this one in your DAW.
  - **"Faderpunk Sim Config"** — the configurator SysEx protocol. The web
    configurator's port discovery (`/faderpunk/i` name match + GetVersion
    probe) finds it like a real device.
- **Transport**: like a fresh device, the clock starts *stopped* — the ▶
  button, Space, or SCENE+SHIFT starts/stops it. The running state persists
  to the FRAM image across restarts. With the clock running, MIDI clock +
  transport stream to the performance port (default global config sends
  both).
- **FRAM**: file-backed image, `fp-sim-fram.bin` in the working directory
  (override with `FP_SIM_FRAM=/path/to/image`). Delete the file for a
  factory-fresh device.
- **Firmware version**: mirrored from `faderpunk/Cargo.toml` at build time,
  so the configurator sees the version this checkout would flash.

## Run

```bash
cargo build -p fp-sim          # note: -p required, fp-sim is not a default member
RUST_LOG=info ./target/debug/fp-sim

# Headless (the phase-1 behavior; Enter toggles transport, q quits):
./target/debug/fp-sim --headless

# Force the LFO app onto channel 0 (useful on an empty layout):
FP_SIM_LFO=1 ./target/debug/fp-sim
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

| Variable          | Effect                                             |
| ----------------- | -------------------------------------------------- |
| `RUST_LOG`        | Log level (`info` default, `debug` for MAX/MIDI)   |
| `FP_SIM_FRAM`     | Path of the FRAM image (default `fp-sim-fram.bin`) |
| `FP_SIM_LFO`      | If set, forces the LFO app (id 2) onto channel 0   |
| `FP_SIM_HEADLESS` | If set, run without the panel window               |

## Architecture notes

The UI is a pure frontend on the main thread (a hard requirement of the
window toolkit on macOS); the embassy executor runs on a background thread.
They meet only through thread-safe statics:

- UI → core: fader positions (`panel::SIM_FADER_POS`), raw button
  press/release transitions (`panel::set_button`), ADC input values
  (`MAX_VALUES_ADC`), transport commands.
- core → UI: rendered LED frame (`hw::LED_FRAME`), DAC values/gate levels
  and port modes (`hw`), latched fader values (`MAX_VALUES_FADER`), clock
  state.

`panel.rs` reproduces the firmware's input semantics (long-press, scene-hold
scene load/save, SHIFT+SCENE transport toggle, `AnalogLatch` fader layers)
against those inputs, so apps see exactly the events they would on hardware.
This boundary is deliberately narrow so the planned parent/child process
split (UI parent, rebuildable headless core child over stdio) is a transport
swap, not a refactor.

## Known limitations

- Only the "USB" MIDI target (index 0) is bridged; DIN 1/2 targets have no
  physical counterpart.
- Windows: `midir` cannot create virtual ports there; use loopMIDI (untested).
- Fader positions are not persisted; sliders start at 0 on launch (on
  hardware the physical fader positions are read at boot).
- Calibration-related behavior (`CALIBRATING`) is untested in the sim.
