# Faderpunk Diagnostics

Local web tool: live MIDI scopes, averaged waveform profiles, and an audible monitor for apps on a connected Faderpunk.

## What it shows

USB MIDI does **not** stream raw CV jack voltages. Most apps can **mirror** their output as Notes/CC on the performance cable when **MidiOut → USB** is enabled in the Configurator.

| Source | Use |
|---|---|
| Config cable (SysEx) | Layout, app list, MIDI channel/CC/routing |
| Performance cable | Live Notes / CC / NRPN / clock |

CV-only apps (Quantizer, Slew, Follower, Offset/Att, AD, MIDI→CV) have no MIDI mirror and will not appear as live scopes unless firmware gains a telemetry protocol later.

## Run

Prerequisites: `./gen-bindings.sh` once (so `configurator/node_modules/@atov/fp-config` exists), then:

```bash
cd faderpunk-diagnostics
pnpm install   # or npm install
pnpm dev       # http://127.0.0.1:3850/
pnpm chrome    # Chromium profile with Web MIDI
```

## Usage

1. On the device, enable **MidiOut → USB** for apps you want to hear/see.
2. **Connect device** (or **Demo mode** without hardware).
3. Views: **All** / **Focus** (one app) / **Compare** (mark apps with **C**).
4. **M** mute, **S** solo (audio + focus), master + wave-rate sliders for the CC→audible wavetable.

Combined audio is the mix of all unmuted (or soloed) tracks.
