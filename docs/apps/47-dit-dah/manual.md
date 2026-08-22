Dit Dah plays packed ASCII Morse phrases as MIDI notes, quantized so each dit/dah starts on a 16th. It needs clock: the first downbeat or a Reset arms playback; after that the phrase loops without waiting for bar resets. Stop silences everything.

Jack is one CV: Gate Out, Pitch Out (0–10 V, V/Oct), or Gate In (±5 V). Gate In rising edge arms the phrase like Shift+short. MIDI notes always fire.

Main sets dit length from a 30 ms floor up to four 16ths of the current tick. Dahs last three dits. Alt transposes the whole phrase. Third (deadzone) adds the Dah interval param only onto dahs. Mute blocks output. Phrase 1 defaults to SOS.
