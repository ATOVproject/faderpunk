Chord Vamp plays MIDI chord progressions in two modes. **Perform**: scrub a **pitch map** of the genre’s unique degrees across ~3 octaves and **hold** the button to sound the chord. **Auto**: a genre **rhythm phrase** (weighted hits + multi-bar rests) loops on the clock while harmony **Repeats** or **Meanders**; **short press** play/pauses. Toggle modes with **Shift + short press**.

In Perform, **Shift + short press** switches to Auto and starts playback. If the always-record ring has chords, it also commits the last N bars (per **Capture length**) into a timed clip. If the ring is empty, Auto runs the **genre phrase** instead. **Perform long press** is unused — the chord stays held while the button is down.

In Auto, **long press** clears the capture clip and reseeds the genre phrase. **Shift + long press** panics. **Shift + short press** returns to Perform.

**Feel** (Button + Fader) scales Auto hit lengths and Capture playback durations. **Swing** (scene storage, genre-seeded) delays odd 16ths; resets when you pick/reseed a genre.

**Jack** folds CV Out, CV In Macro, and CV In Panic into one enum — Macro modulates scrub (Perform) or tension (Auto); Panic fires All Notes Off from CV.
