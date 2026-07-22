# Custom MIDI CC CSVs

Drop your own device CSVs here. They appear in the editor catalog as `Custom/…`
and are **never** overwritten when the Pencil Research database is updated.

Layout (same as [pencilresearch/midi](https://github.com/pencilresearch/midi)):

```
midi-custom/
  MyMaker/
    My Device.csv
```

Use their [template.csv](https://raw.githubusercontent.com/pencilresearch/midi/main/template.csv)
for columns (`cc_msb`, `parameter_name`, `section`, …).

Browse the public DB at https://midi.guide/
