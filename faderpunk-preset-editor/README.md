# Faderpunk Preset Editor

Local preset-bank editor for Faderpunk layouts, app parameters, MIDI routing,
global configuration and instrument definitions.

**Canonical location** inside the Faderpunk repo (`faderpunk-preset-editor/`).
The older standalone `faderpunk-scenes` repo is deprecated.

The editor ships empty. Use **Pull from Punk** to read the connected device, or
build a preset manually. Presets are persisted both in browser storage and in
`out/preset-bank.json` by the local server.

Works with the **beta** configurator (`https://faderpunk.io/beta`). Override with
`FP_CONFIG_URL` or `FP_CONFIG_PREFER=local|beta|official`.

## Start

```bash
cd faderpunk-preset-editor
npm install
npm start
```

Open http://127.0.0.1:3847/.

## Pull and push

- **Pull from Punk** reads the current layout, parameters and global
  configuration through the Configurator.
- **Push to Punk** loads the active editor preset into the Configurator.

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

[AGPL-3.0](LICENSE). Included third-party assets:

- Icons in `icons/` come from the
  [ATOVproject/faderpunk](https://github.com/ATOVproject/faderpunk)
  Configurator (GPL-3.0), icon design by papernoise.
- The Martian Mono font in `fonts/` is licensed under the
  [SIL Open Font License](fonts/OFL.txt).
