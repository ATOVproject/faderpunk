# Faderpunk Preset Editor

Local preset-bank editor for Faderpunk layouts, app parameters, MIDI routing,
global configuration and instrument definitions.

The editor ships empty. Use **Pull from Punk** to read the connected device, or
build a preset manually. Presets are persisted both in browser storage and in
`out/preset-bank.json` by the local server.

## Start

```bash
npm install
npm start
```

Open http://127.0.0.1:3847/.

## Pull and push

- **Pull from Punk** reads the current layout, parameters and global
  configuration through the Configurator.
- **Push to Punk** loads the active editor preset into the Configurator.

The editor automatically uses a local Configurator on port 5173 when available,
otherwise the hosted beta Configurator. Override this with `FP_CONFIG_URL`.

The device must be connected in the dedicated Chrome window. Launch it with:

```bash
npm run chrome
```

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

[GPL-3.0](../LICENSE), like the rest of this repository. Icons in `icons/`
are copies from `configurator/public/icons/` (icon design by papernoise).
The Martian Mono font in `fonts/` is licensed under the
[SIL Open Font License](fonts/OFL.txt).
