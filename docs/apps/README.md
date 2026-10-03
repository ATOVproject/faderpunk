# Per-app UX / manual sources

Canonical documentation fragments for Faderpunk apps. Feature branches own
`docs/apps/<id>-<slug>/`; assemblers produce the unified cheatsheet (and later
Configurator manuals).

## Layout

```
docs/apps/<id>-<slug>/
  manual.json   # structured metadata, gestures, params, channels
  manual.md     # prose body (cheatsheet .manual + future ManualTab text)
```

`id` is the registry app ID. `slug` matches the firmware module name with
underscores as hyphens (`loop_de_cay` → `loop-de-cay`).

## Status

| `status` | Meaning |
| --- | --- |
| `wip` | Playground / feature WIP |
| `stock` | Shipped on main (docs-only here; do not edit stock firmware) |

| `tag` | Cheatsheet badge |
| --- | --- |
| `ready` | Stable enough for hardware testing |
| `draft` | In flux |

## Pilots

Currently fragment-backed: **32 Super LFO**, **37 Harmonica**, **38 Loop de Cay**,
**40 Venn**, **41 Bassment**, **42 Contura**.
Other WIP apps still live as HTML under `docs/cheatsheet/legacy/` until migrated.

## Build

```bash
python3 scripts/assemble-cheatsheet.py          # → docs/wip-apps-ux-cheatsheet.html
python3 scripts/assemble-cheatsheet.py --pdf    # also regenerate PDF (Chrome)
python3 scripts/validate-app-docs.py            # fragment / CONFIG checks
python3 scripts/export-scopepunk-ux.py          # → faderpunk-tools/scopepunk/public/app-ux.json (all)
python3 scripts/export-scopepunk-ux.py --scope wip|stock
```

Sources (merge priority): `docs/apps/*/manual.json` → `docs/cheatsheet/legacy/*.html` → Configurator `ManualTab.tsx` (read-only; stock channels).

## Future Configurator manuals

`manual.json` fields (`params`, `storage`, `channels`, `color`, `icon`) mirror
`ManualAppData` in `configurator/src/components/manual/ManualApp.tsx`. A later
generator will emit ManualTab data from these fragments without putting prose
in firmware. Do **not** commit `configurator/` unless explicitly asked.
