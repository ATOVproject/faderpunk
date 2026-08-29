Dit Dah plays packed ASCII Morse phrases as MIDI notes, quantized so each dit/dah starts on a 16th. It needs clock: MIDI Start/Reset or the first 16th after ticks appear (analog clock on Atom/Meteor/Cube has no transport) arms playback; after that the phrase loops without waiting for bar resets. Stop silences everything.

Jack is one CV: CV Out Gate, CV Out Pitch (0–10 V, V/Oct), CV In Gate (±5 V rising edge arms like long press), or CV In Dit length / Pitch offset / Dah interval. CV In dests add ±5 V bipolar on top of the saved fader at playback — not written to storage. Unpatched ±5 V ≈ centre, so no change. MIDI notes always fire.

Main sets dit length from a 30 ms floor up to four 16ths of the current tick. Dahs last three dits. Alt transposes the whole phrase. Third (deadzone) adds the Dah interval param only onto dahs. ITU gaps (1 / 3 / 7) stay; invert only swaps tone lengths and flips the fader mappings at playback.

Short press mutes (Button LED off). Shift+short toggles texture: random MIDI velocity and swing on odd 16ths (Bottom LED white). Long press retriggers — arms the next 16th like CV In Gate. Shift+long inverts dit↔dah and flips speed/pitch at playback (Button LED white when unmuted; orange when normal). Phrase 1 defaults to SOS.
