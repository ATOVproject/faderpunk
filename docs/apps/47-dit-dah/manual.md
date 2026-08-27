Dit Dah plays packed ASCII Morse phrases as MIDI notes, quantized so each dit/dah starts on a 16th. It needs clock: MIDI Start/Reset or the first 16th after ticks appear (analog clock on Atom/Meteor/Cube has no transport) arms playback; after that the phrase loops without waiting for bar resets. Stop silences everything.

Jack is one CV: CV Out Gate, CV Out Pitch (0–10 V, V/Oct), CV In Gate (±5 V rising edge arms like Shift+short), or CV In Dit length / Pitch offset / Dah interval. CV In dests add ±5 V bipolar on top of the saved fader at playback — not written to storage. Unpatched ±5 V ≈ centre, so no change. MIDI notes always fire.

Main sets dit length from a 30 ms floor up to four 16ths of the current tick. Dahs last three dits. Alt transposes the whole phrase. Third (deadzone) adds the Dah interval param only onto dahs. Mute blocks output. Phrase 1 defaults to SOS.
