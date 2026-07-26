# Faderpunk Preset Editor

Local preset-bank editor for Faderpunk layouts, app parameters, MIDI routing,
global configuration and instrument definitions.

**Canonical location** inside the Faderpunk repo (`faderpunk-preset-editor/`).

The older standalone `faderpunk-scenes` repo is retired. Local copy (if any) lives
under `~/Projects/_archived/faderpunk-scenes`; a stub at `~/Projects/faderpunk-scenes`
only points here. Do not run or edit that tree.

The editor ships empty. Use **Pull from Punk** to read the connected device, or
build a preset manually. Presets are persisted both in browser storage and in
`out/preset-bank.json` by the local server.

Pull and push talk to the device **directly over Web MIDI SysEx** (config cable) —
no Configurator tab and no debug Chrome / CDP.

## Start

```bash
cd faderpunk-preset-editor
npm start
```

Open http://127.0.0.1:3847/ in a Chromium browser (SysEx permission required).

Close other tabs that hold the Faderpunk MIDI ports (Configurator, Diagnostics,
etc.) — Web MIDI is exclusive on macOS.

## Pull and push

- **Pull from Punk** reads layout, app parameters and global config via SysEx.
- **Push to Punk** writes the active editor preset with the same sequence as the
  Configurator Recall flow (`SetLayout` → settle → `SetAppParams` → `SetGlobalConfig`).

## Instruments and MIDI CC data

Instrument definitions are user-created and stored locally. A pull can associate
rows with instruments when their MIDI channel (and, when needed, CC) is
unambiguous.

The editor downloads the public
[pencilresearch/midi](https://github.com/pencilresearch/midi) database on first
use. Use **Fetch CCs online** to refresh it or **Upload CSV** to add a private
file under `midi-custom/`. Downloaded and uploaded CSV data is not committed.

## Checks

```bash
npm run check
```

## License

[AGPL-3.0](LICENSE). Included third-party assets:

- Icons in `icons/` come from the
  [ATOVproject/faderpunk](https://github.com/ATOVproject/faderpunk)
  Configurator (GPL-3.0), icon design by papernoise.
- The Martian Mono font in `fonts/` is licensed under the
  [SIL Open Font License](fonts/OFL.txt).
- `vendor/fp-config/` is the generated `@atov/fp-config` postcard binding from
  this repo’s `gen-bindings` output (regenerate with `./gen-bindings.sh` and
  re-copy if protocol types change).
