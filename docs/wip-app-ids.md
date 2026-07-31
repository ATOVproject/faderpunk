# WIP app ID registry

Claim an ID here **before** adding `register_apps!` on a feature or playground
branch. Keep this table in sync when starting or retiring a WIP app.

Stock apps on `main` use IDs **1–27** (plus any later IDs merged to `main`).
Do not reuse those.

Flash integration branch: **`test/playground`** (count-agnostic; formerly
`test/all-five-apps`).

| ID | App | Feature branch | Notes |
|---:|---|---|---|
| 28 | Sift | (upstream / sift branches) | |
| 29 | Heat Pump | `heat_pump` | |
| 30 | Grooves | `feat/grooves` | needs `ParamStore::update` — see patches |
| 31 | Golden Gate | `feat/golden-gate` | aka Fibonacci Gate on some branches |
| 32 | Super LFO | `feat/super-lfo-app` | Mix balance 0–100%; CV Dest select; Alt0=Attenuation |
| 33 | Echolot | `feat/echolot` | MIDI/CV delay |
| 34 | Arp de Lévy | `feat/arp-de-levy` | |
| 35 | Chord Vamp | `feat/vamp` | Chord progressions (MIDI + optional root CV out) |
| 36 | Hold Sam | `feat/hold-sam` | Clocked S&H CV → MIDI Note/CC |
| 37 | Harmonica | `feat/harmonica` | Monophonic MIDI/CV harmonizer |

Next free WIP ID: **38**
