import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";

import { Preface } from "./manual/Preface";
import { type ManualAppData } from "./manual/ManualApp";
import { UpdateGuide } from "./manual/UpdateGuide";
import { Troubleshooting } from "./manual/Troubleshooting";
import { Apps } from "./manual/Apps";
import { H2, List, Link } from "./manual/Shared";
import { Interface } from "./manual/Interface";
import { Configurator } from "./manual/Configurator";

const apps: ManualAppData[] = [
  {
    appId: 1,
    title: "Control",
    description: "Simple MIDI/CV controller",
    color: "Violet",
    icon: "fader",
    params: [
      "Curve",
      "Range",
      "MIDI Channel",
      "MIDI CC",
      "Mute on release",
      "Invert",
      "Color",
      "Store state",
      "Button mode",
      "Button Channel",
      "Button CC / PC",
      "NRPN",
    ],
    storage: [
      "Level (if 'Store state' enabled)",
      "Muted (if 'Store state' enabled)",
      "Attenuation",
    ],
    text: "This app is designed to provide a simple way to manually control any parameters using either CV or MIDI CC. The MIDI channel and CC numbers can be adjusted in the app's settings, and both MIDI and CV outputs are always active simultaneously. The range can be adjusted using Shift + Fader, which affects both CC and CV ranges. The fader controls the level of the CV or CC. The button behavior is set by the Button mode parameter: Mute toggles the output on and off; CC toggle sends a CC value that alternates between 0 and 127; CC momentary sends 127 while held and 0 on release; Program Change sends a MIDI Program Change message on press using the Button CC / PC number as the program number—useful for switching presets on external devices. The fader level and the mute state can be saved in scenes if the 'Store state' parameter is enabled (active by default). You can then use this app as a way to save and recall CV voltage allowing for preset in a modular system for example. The curve can be adjusted in the settings; however, this only affects the CV output. Two voltage ranges are available in the settings: 0V to 10V or -5V to 5V. Note that this range also affects the level at which CV and CC are set when muting. In the 0V to 10V range, mute is at 0V and CC 0, making it ideal for controlling volume, send levels, or similar parameters. In the -5V to 5V range, mute is at 0V and CC 64, making it suitable for controlling panning, crossfading, or similar functions. The mute behavior can be set to trigger on press or on release, depending on your preference. Due to popular demand, the app's action can also be inverted—this means that when the fader is at the top, the output will be set to the minimum value, and when at the bottom, it will send the maximum CC and CV value. As with all apps where the LED color does not serve any specific function, you are free to configure it in the settings.",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "CV Output",
        faderTitle: "Sets CV and MIDI CC value",
        faderDescription: "",
        faderPlusShiftTitle: "Attenuation level",
        faderPlusShiftDescription: "Reduces the CV and CC range",
        fnTitle: "Mute",
        fnDescription: "",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
      },
    ],
  },

  {
    appId: 2,
    title: "LFO",
    description: "Multi shape LFO",
    color: "Yellow",
    icon: "sine",
    params: [
      "Speed division",
      "Range",
      "MIDI Channel",
      "MIDI CC",
      "NRPN",
      "Send MIDI",
      "Grid Lock",
    ],
    storage: ["Clocked", "Attenuation", "Speed", "Waveform", "Muted"],
    text: `LFO is a multi-shape oscillator with manually selectable waveforms. Press the button to cycle through shapes; the LED color shows the active waveform: sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), square (white).

#### Speed and range

The fader sets the LFO rate. In free-running mode, speed ranges from 14 Hz down to one cycle per minute. **Shift + Fader** adjusts output attenuation, reducing the CV amplitude. The **Speed** parameter applies a global multiplier—Normal, Slow (÷2), Slowest (÷4)—that works in both free-running and clocked modes.

#### Clocked mode

**Shift + long press** toggles between free-running and tempo-synced modes. When clocked, the available resolutions are: 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, and bar. The button flashes in sync with the LFO rate. **Shift + short press** resets the LFO phase to zero. **Long press** (no shift) mutes the output.

#### Grid Lock

**Grid Lock only has an effect in clocked mode.** When enabled (default on), the LFO phase is continuously derived from the clock's absolute tick count. The phase is always correct relative to the clock grid regardless of when the LFO was started or reset. Changing the speed division re-aligns to the new grid automatically, but this can cause a click as the LFO jumps to its recalculated position.

A **Shift + short press** offsets the phase reference to the current tick, drifting the LFO out of phase with the grid for creative offset effects. A clock reset re-locks to the grid.

Disabling Grid Lock reverts to free-running phase accumulation: the LFO will smoothly speed up or slow down from its current position when the division changes, with no jump.

#### Output

The output range is configured in the parameters: bipolar (−5V to +5V) or unipolar (0V to 10V). This also sets the MIDI CC center—64 for bipolar, 0 for unipolar. MIDI channel and CC number are freely configurable.`,
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "-5V to 5V LFO out",
        faderTitle: "LFO speed",
        faderDescription:
          "Sets the LFO speed, top is maximum and bottom slowest",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range",
        fnTitle: "Waveform / Mute",
        fnDescription: "Short: cycle waveform. Long (no shift): mute output",
        fnPlusShiftTitle: "Reset / Clocked mode",
        fnPlusShiftDescription: "Short: reset LFO. Long: toggle clocked mode",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
      },
    ],
  },
  {
    appId: 3,
    title: "AD envelope",
    description: "Variable curve AD, ASR or looping AD",
    color: "Yellow",
    icon: "ad-env",
    params: ["Use MIDI", "MIDI Channel", "MIDI retrigger"],
    storage: [
      "AD lengths",
      "AD curves",
      "Mode",
      "Attenuation",
      "Trigger to gate timing",
      "Muted",
    ],
    text: "This is a multimode envelope generator offering AD, ASR, and looping AD modes. Using the buttons, Attack and Decay curves are individually adjustable. Shift + Button 2 switches between modes: AD (yellow), ASR (blue), and looping AD (pink). Shift + Button 1 provides a manual trigger, Shift + Fader 1 sets the trigger-to-gate timing, and Shift + Fader 2 controls attenuation. Long press on Button 2 (no shift) mutes the envelope output. The envelope can also be triggered via MIDI, with the MIDI channel set in the parameters. An internal trigger-to-gate converter defines how long the gate stays active, ranging from 0 to 4 seconds—at maximum time, the gate remains on indefinitely. This timing behaves differently depending on the selected envelope mode: in AD mode, it prevents retriggering until the timer runs out; in ASR mode, it holds the envelope for the set duration; and in looping AD mode, it loops the envelope for the timer duration, with infinite looping at maximum time, effectively turning it into an LFO. MIDI note triggering is supported on a user-defined channel, allowing you to save channels by using MIDI directly instead of relying on a MIDI-to-CV gate. The 'MIDI retrigger' parameter allow for the envelope to be retriggered when MIDI notes are overlapping",
    channels: [
      {
        jackTitle: "Gate Input",
        jackDescription: "Gate is detected if the voltage is above 1V",
        faderTitle: "Attack time",
        faderDescription: "Sets the attack time from 0 to 4 sec",
        faderPlusShiftTitle: "Trigger to gate time",
        faderPlusShiftDescription: "0-4 sec. Infinite at maximum.",
        fnTitle: "Attack curve",
        fnDescription: "Short: cycle attack curve",
        ledTop: "Output level in attack phase",
        ledTopPlusShift: "Trigger to gate time (flash)",
        ledBottom: "Gate input state",
        fnPlusShiftTitle: "Manual trigger",
      },
      {
        jackTitle: "Envelope Output",
        jackDescription: "0-10V output range",
        faderTitle: "Decay time",
        faderDescription: "Sets the decay time from 0 to 4 sec",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range.",
        fnTitle: "Decay curve / Mute",
        fnDescription: "Short: cycle decay curve. Long (no shift): mute output",
        ledTop: "Output level in decay phase",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "inactive",
        fnPlusShiftTitle: "Envelope mode",
        fnPlusShiftDescription:
          "AD (yellow), ASR (blue), and looping AD (pink)",
      },
    ],
  },
  {
    appId: 4,
    title: "Random CC/CV",
    description: "Generate random CC and CV values",
    color: "Green",
    icon: "random",
    params: ["Range", "MIDI Channel", "MIDI CC", "NRPN"],
    storage: ["Speed", "Muted", "Attenuation", "Slew", "Clocked"],
    text: "This app sends random CC and CV values at regular intervals, either in free-running mode or synced to a clock. The timing is set using the fader, and the MIDI channel and CC number can be configured in the parameters. Shift + Fader attenuates both CV and CC outputs, while Button + Fader accesses the onboard slew limiter, which smooths changes in both CV and CC values. Button (no shift) toggles mute/unmute. The output range can be set to unipolar or bipolar in the parameters, which also determines the mute behavior—settling at 0 in unipolar mode and in the middle in bipolar mode, similar to the Control app. Shift + long press switches between free-running and tempo-synced operation.",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "Either -5V to 5V or 0 to 10V CV",
        faderTitle: "Speed",
        faderDescription: "Sets the speed, top is maximum and bottom slowest",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range",
        faderPlusFnTitle: "Slew",
        faderPlusFnDescription: "Slew limiter timing.",
        fnTitle: "Mute",
        fnDescription: "Short press (no shift) toggles mute",
        fnPlusShiftTitle: "Clocked mode",
        fnPlusShiftDescription: "Long press: toggle free-running/clocked",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledTopPlusFn: "Slew level in green",
        ledBottom: "Negative level indicator",
      },
    ],
  },
  {
    appId: 5,
    title: "Sequencer",
    description: "4 x 16 step CV/gate sequencers",
    color: "Yellow",
    icon: "sequence",
    params: [
      "MIDI Channel 1",
      "MIDI Channel 2",
      "MIDI Channel 3",
      "MIDI Channel 4",
      "MIDI Out",
      "Track 2: velocity lane",
      "Track 4: velocity lane",
      "Transpose MIDI In",
      "Track 1: transpose CH",
      "Track 2: transpose CH",
      "Track 3: transpose CH",
      "Track 4: transpose CH",
    ],
    storage: [
      "Sequences (Gate/CV)",
      "Legato",
      "Sequence lengths",
      "Gate lengths",
      "Octaves",
      "Ranges",
      "Sequence resolutions",
      "Directions",
      "Probabilities",
      "Slide times",
      "Muted (per track)",
    ],
    text: "4x16 step sequencer app featuring four independent sequencers, each represented by a distinct color. Each sequencer has two pages, navigated with **Shift + Buttons** (short press). CV/Gate outputs are paired per sequencer: jacks 1&2 for sequencer 1, 3&4 for sequencer 2, 5&6 for sequencer 3, and 7&8 for sequencer 4. Faders set note values, buttons define the gate pattern, and **long button presses** enable legato between steps. CV output is quantized to the scale set in the global quantizer.\n\n#### Shift mode\n\nWith Shift held, all 8 faders control settings for the currently selected sequencer:\n\n- **Fader 1** — step length (1–16 steps)\n- **Fader 2** — gate length\n- **Fader 3** — octave offset (0–5 octaves)\n- **Fader 4** — sequence range (1–5 octaves)\n- **Fader 5** — clock resolution (32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th)\n- **Fader 6** — direction (Forward / Backward / Ping-Pong / Random)\n- **Fader 7** — trigger probability (5%–100%)\n- **Fader 8** — 303-style slide time (0 = instant)\n\nTop LEDs show sequence length. Resolution LED color: **orange** = triplet division, **blue** = straight division.\n\n#### Muting tracks\n\n**Shift + long press** on an even button (0, 2, 4, or 6) mutes the corresponding track (1–4). While muted, gates and CV are suppressed and the top and position LEDs turn off; the gate pattern remains visible so the sequence can still be edited. Mute state is saved per scene.\n\n#### MIDI transposition\n\nEach track can be live-transposed via incoming MIDI. Assign a dedicated transpose MIDI channel per track in the parameters. Receiving a NoteOn on that channel shifts the track's output relative to **C4** — middle C = no change, notes above/below transpose up/down by semitones. Transposition applies to both CV and MIDI output.\n\n#### Velocity lane mode (tracks 2 and 4)\n\nWhen enabled, a track acts as a velocity lane for its paired primary track (track 2 → track 1, track 4 → track 3):\n\n- **Fader** — controls the MIDI velocity sent to the paired track, not a note value\n- **CV output** — unquantized voltage proportional to fader position (0–10V)\n- **Gate button** — controls whether velocity advances on that step (on) or holds the previous value (off)\n- MIDI notes are suppressed on the velocity lane track itself",
    channels: [
      {
        jackTitle: "CV Output",
        jackDescription: "Quantized note output",
        faderTitle: "Note / Velocity",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Sequence length",
        faderPlusShiftDescription:
          "Set the length of the selected sequencer between 1 and 16 steps",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Seq 1 page 1 / Mute Seq 1",
        fnPlusShiftDescription:
          "Short: select page. Long: mute/unmute sequencer 1",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Gate output",
        faderTitle: "Note / Velocity",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Gate length",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 1, page 2",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "CV Output",
        jackDescription:
          "Quantized note output, or unquantized velocity CV when velocity lane is enabled",
        faderTitle: "Note / Velocity",
        faderDescription:
          "Sets the note at this step, or velocity level when velocity lane is enabled",
        faderPlusShiftTitle: "Octave",
        faderPlusShiftDescription: "Offset the whole sequence by 0–5 octaves",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Seq 2 page 1 / Mute Seq 2",
        fnPlusShiftDescription:
          "Short: select page. Long: mute/unmute sequencer 2",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Gate output",
        faderTitle: "Note / Velocity",
        faderDescription:
          "Sets the note at this step, or velocity level when velocity lane is enabled",
        faderPlusShiftTitle: "Sequence Range",
        faderPlusShiftDescription: "Set sequence range (1–5 octaves)",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 2, page 2",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "CV Output",
        jackDescription: "Quantized note output",
        faderTitle: "Note / Velocity",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Sequence resolution",
        faderPlusShiftDescription:
          "Set sequence resolution: 32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Seq 3 page 1 / Mute Seq 3",
        fnPlusShiftDescription:
          "Short: select page. Long: mute/unmute sequencer 3",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Gate output",
        faderTitle: "Note / Velocity",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Direction",
        faderPlusShiftDescription:
          "Set sequence direction: Forward, Backward, Ping-Pong, or Random",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 3, page 2",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "CV Output",
        jackDescription:
          "Quantized note output, or unquantized velocity CV when velocity lane is enabled",
        faderTitle: "Note / Velocity",
        faderDescription:
          "Sets the note at this step, or velocity level when velocity lane is enabled",
        faderPlusShiftTitle: "Probability",
        faderPlusShiftDescription:
          "Set trigger probability for this sequencer (5%–100%)",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Seq 4 page 1 / Mute Seq 4",
        fnPlusShiftDescription:
          "Short: select page. Long: mute/unmute sequencer 4",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Gate output",
        faderTitle: "Note / Velocity",
        faderDescription:
          "Sets the note at this step, or velocity level when velocity lane is enabled",
        faderPlusShiftTitle: "Slide time",
        faderPlusShiftDescription:
          "Set 303-style portamento slide time (0 = instant)",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 4, page 2",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
    ],
  },
  {
    appId: 6,
    title: "Turing",
    description: "Turing machine, synched to internal clock",
    color: "Blue",
    icon: "sequence-square",
    params: [
      "MIDI mode",
      "Midi channel",
      "CC number",
      "Base Note",
      "GATE %",
      "Color",
      "Range",
      "NRPN",
      "MIDI Out",
      "Gate Out",
    ],
    storage: [
      "Attenuation",
      "Length",
      "Register",
      "Resolution",
      "Gate threshold mode",
      "Muted",
    ],
    text: "A probabilistic sequencer inspired by the Turing machine concept in modular synthesis. A shift register of up to 16 bits evolves over time based on a probability setting, generating melodic and rhythmic patterns that can stay fixed, drift slowly, or change continuously. CV output is always active and quantized to the global quantizer scale. MIDI CC or MIDI notes can be sent simultaneously.\n\n#### Probability\n\nThe **fader** controls the likelihood of bit flips on each clock tick:\n\n- **Bottom** — no flips, the sequence repeats identically\n- **Middle** — 50/50 chance per step, maximum randomness\n- **Top** — constant flips; the effective sequence length also doubles\n\nThe top LED brightness reflects the current output level. A white flash on the bottom LED marks the start of each sequence cycle.\n\n#### Mute\n\nA **short press** (without Shift) toggles mute. When muted, the CV output holds its last value rather than dropping to 0, and the button LED turns off. Mute state is saved per scene.\n\n#### Sequence length\n\nHold **Shift** and press the button once per step to record a new length. Each press increments the count; releasing Shift commits it. For example: hold Shift, press three times, release → 3-step sequence. The sequence register and length are both saved per scene.\n\n#### Clock resolution\n\nHold the **button** and move the **fader** to set the clock resolution: 32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th. While adjusting, the bottom LED shows the division type — **orange** for triplets, **blue** for straight.\n\n#### Shift layer\n\nHolding **Shift** activates a second fader layer. The fader now controls **attenuation**, which reduces the CV output range and — in Gate Out mode — doubles as a gate density control. The top LED turns red and reflects the attenuation level.\n\n#### Gate Out mode\n\nWhen **Gate Out** is enabled in the parameters, the output jack outputs a gate signal instead of CV. Two sub-modes are available, toggled at runtime with **Shift + Long press**:\n\n- **Threshold mode** (default, button LED **yellow** while Shift held) — gate fires when the register value falls below the attenuation level. Higher attenuation = fewer gates; lower attenuation = denser gates.\n- **Bit mode** (button LED **blue** while Shift held) — gate fires when the output bit of the shift register is high, producing rhythmic patterns locked to the register’s content. The attenuation level has no effect in this mode.\n\nIn Gate Out mode, MIDI sends the **Base Note** parameter on each gate-on event rather than a quantized pitch.",
    channels: [
      {
        jackTitle: "CV / Gate Output",
        jackDescription:
          "0–10V quantized CV (Gate Out off) or gate signal (Gate Out on)",
        faderTitle: "Probability",
        faderDescription:
          "Bottom: no bit flip, Top: constant bit flips and doubled sequence length; Middle: max randomness",
        faderPlusShiftTitle: "Attenuation / Gate density",
        faderPlusShiftDescription:
          "Reduces CV output range; in Gate Out mode controls gate density (threshold mode)",
        faderPlusFnTitle: "Speed",
        faderPlusFnDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th",
        fnTitle: "Mute",
        fnDescription: "Short press (no shift) mutes/unmutes the output",
        fnPlusShiftTitle: "Sequence Length",
        fnPlusShiftDescription:
          "Short press x times while holding Shift sets length to x. Long press (Gate Out on): toggle threshold / bit gate mode",
        ledTop:
          "CV output level (CV mode) / Gate state — on/off (Gate Out mode)",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "White flash at sequence repeat point",
        ledBottomPlusShift: "Flash at tempo",
      },
    ],
  },
  {
    appId: 7,
    title: "Turing+",
    description: "Turing machine, with clock input",
    color: "Orange",
    icon: "euclid",
    params: [
      "MIDI mode",
      "Midi channel",
      "CC number",
      "Color",
      "Range",
      "Base note",
      "NRPN",
    ],
    storage: ["Attenuation", "Length", "Register", "Muted"],
    text: "Similar to the previous one, this is a classic Turing machine but extended to use two slots. The first jack is a clock input and the second is the CV output. The physical clock input allows for non-linear timing, custom dividers, or interaction with MIDI note lengths. The app can send either MIDI CC or MIDI notes, while CV output is always active, sending 0–10V. MIDI note on messages are sent on rising edges and note off messages on falling edges. Main functions: Fader 1 sets probability, Fader 2 sets output range. Shift + Button sets sequence length. Short press (no shift) on button 2 mutes the output; the CV holds its last value rather than dropping to 0. The output is quantized by the global quantizer.",
    channels: [
      {
        jackTitle: "Gate input",
        jackDescription: "Gate is detected if the voltage is above 1V",
        faderTitle: "Probability",
        faderDescription:
          "Bottom: no bit flip, Top: constant bit flips and doubled sequence length; Middle: max randomness",
        fnPlusShiftTitle: "Sequence Length",
        fnPlusShiftDescription: "Press button x times sets length to x",
        ledTop: "Pre attenuation level",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Gate input indicator",
      },
      {
        jackTitle: "Output",
        jackDescription: "0 to 10V CV",
        faderTitle: "Attenuation",
        faderDescription: "Reduces the output range",
        fnTitle: "Mute",
        fnDescription: "Short press (no shift) mutes/unmutes the output",
        ledTop: "Output level indicator",
        ledBottom: "",
      },
    ],
  },

  {
    appId: 8,
    title: "Euclid",
    description: "Euclidean sequencer",
    color: "Orange",
    icon: "euclid",
    params: ["MIDI Channel", "MIDI NOTE 1", "MIDI NOTE 2", "GATE %", "Color"],
    storage: [
      "Length",
      "Fill",
      "Rotation",
      "Speed",
      "Probability",
      "Muted",
      "Mode",
    ],
    text: "This app is a Euclidean sequencer with two outputs: Jack 1 delivers the main Euclidean rhythm, while Jack 2 provides either an inverted version or an end-of-rhythm pulse. In inverted mode, if Output 1 sends a pulse, Output 2 does not—and vice versa. Send MIDI triggers, with MIDI channel and MIDI notes. Main functions include Fader 1 for sequence length and Fader 2 for number of beats (fill). Button 1 toggles semitone offset, Button 2 mutes the output. Shift + Fader 1 sets rotation, Shift + Fader 2 sets probability. Button + Fader 1 changes the sequencer speed with available resolutions: 32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar. While setting speed, LED color indicates division type: orange for triplet divisions and blue for straight divisions.",
    channels: [
      {
        jackTitle: "Trigger 1 Out",
        jackDescription: "Outputs 10V triggers",
        faderTitle: "Length",
        faderDescription: "Sets the length of the sequence",
        faderPlusShiftTitle: "Rotation",
        faderPlusShiftDescription: "Rotates the sequence",
        faderPlusFnTitle: "Speed",
        faderPlusFnDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar",
        fnTitle: "Speed",
        fnDescription: "Fn + Fader changes the sequencer speed",
        ledTop: "Trigger 1 activity",
        ledBottom: "",
        ledBottomPlusFn: "Clock speed (orange: triplet, blue: straight)",
      },
      {
        jackTitle: "Trigger 2 Out",
        jackDescription: "Outputs 10V triggers",
        faderTitle: "Beats",
        faderDescription: "Amount of beats in the sequence",
        faderPlusShiftTitle: "Probability",
        faderPlusShiftDescription:
          "Chances that the sequencer outputs a trigger",
        fnTitle: "Mute",
        fnDescription: "Mute the sequencer",
        fnPlusShiftTitle: "Mode switch",
        fnPlusShiftDescription: "Set output 2 to inverted mode or EoC",
        ledTop: "Trigger 1 activity",
        ledBottom: "",
      },
    ],
  },
  {
    appId: 9,
    title: "Random Trigger",
    description: "Sends random triggers on clock",
    color: "Cyan",
    icon: "die",
    params: ["MIDI Channel", "MIDI NOTE", "GATE %", "Color"],
    storage: ["Probability", "Muted", "Resolution"],
    text: "This app sends random trigger signals on clock. It can output MIDI notes and CV triggers. The fader sets the probability of a trigger occurring at each clock pulse. The button acts as a mute toggle. Shift + Fader sets the clock resolution, allowing for rhythmic variation. While adjusting resolution, the bottom LED is orange for triplet divisions and blue for straight divisions.",
    channels: [
      {
        jackTitle: "Trigger Output",
        jackDescription: "Sends random triggers on clock",
        faderTitle: "Probability",
        faderDescription: "Sets the chance of a trigger on each clock pulse",
        faderPlusShiftTitle: "Resolution",
        faderPlusShiftDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar, 2 bars, 4 bars",
        fnTitle: "Mute",
        fnDescription: "Toggles trigger output on/off",
        fnPlusShiftTitle: "",
        fnPlusShiftDescription: "",
        ledTop: "Trigger activity indicator",
        ledBottom: "Flashes with clock",
        ledBottomPlusShift: "Resolution type (orange: triplet, blue: straight)",
      },
    ],
  },

  {
    appId: 10,
    title: "Note Fader",
    description: "Play MIDI notes manually or on clock",
    color: "Rose",
    icon: "note",
    params: ["MIDI Channel", "Base note", "Span", "GATE %", "Out", "Color"],
    storage: ["Note", "Resolution", "Muted", "Clocked"],
    text: "This app sends MIDI notes and V/Oct voltages in a 0–10V range. The outputted notes are filtered by the global quantizer. The note value is tied to the fader position, with the range set by the span parameter. In clocked mode, the button is a toggle and the app outputs notes on regular intervals set by Button + Fader. In direct mode, the MIDI notes are sent when the button is pressed. Main functions: Fader sets the note; Shift + Fader sets clock resolution; Shift + Button toggles mode—Bottom LED is flashing for clocked mode, off for direct mode. Resolution type is color-coded as orange for triplet divisions and blue for straight divisions.",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "Sends either V/Oct or Gate signal",
        faderTitle: "Note",
        faderDescription: "Sets the note value based on fader position",
        faderPlusShiftTitle: "Resolution",
        faderPlusShiftDescription:
          "Sets clock resolution: 32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar",
        fnTitle: "Mode",
        fnDescription: "Direct mode trigger note, clocked mode toggles",
        fnPlusShiftTitle: "Toggles between clocked and direct mode",
        fnPlusShiftDescription: "",
        ledTop: "Note output indicator",
        ledBottom: "Flashes in clocked mode (orange: triplet, blue: straight)",
      },
    ],
  },
  {
    appId: 11,
    title: "Offset + Attenuverter",
    description: "Offset and attenuverter module",
    color: "Rose",
    icon: "attenuate",
    params: ["Color", "Range"],
    storage: ["Attenuation", "Offset", "Offset toggle", "Attenuation toggle"],
    text: "This app provides offset and attenuverter functionality. The input and output range is configurable as either ±5V (default) or 0–10V via the Range parameter, and the attenuverter has a maximum gain of 2x. Color can be set in the configurator. Jack 1 is the input, Jack 2 is the output. Main functions include Fader 1 for offset and Fader 2 for attenuvertion. Button 1 toggles the offset on or off, Button 2 toggles the attenuvertion on or off. When both of these are off the app acts as a simple pass through.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V or 0–10V signals (set by Range)",
        faderTitle: "Offset",
        faderDescription: "Applies a DC offset to the input signal",
        fnTitle: "Kill Offset",
        fnDescription: "Button 1 disables the offset",
        ledTop: "Positive input",
        ledBottom: "Negative input",
      },
      {
        jackTitle: "Output",
        jackDescription: "Outputs ±5V or 0–10V signals (set by Range)",
        faderTitle: "Attenuverter",
        faderDescription: "Scales and inverts the input signal (max gain 2x)",
        fnTitle: "Kill Attenuverter",
        fnDescription:
          "Button 2 disables the attenuvertion and set to unity gain",
        ledTop: "Positive output",
        ledBottom: "Negative output",
      },
    ],
  },
  {
    appId: 12,
    title: "Slew Limiter",
    description: "Slows CV changes with offset and attenuverter",
    color: "Green",
    icon: "soft-random",
    params: ["Color", "Range"],
    storage: ["Attack", "Attenuvertion", "Offset"],
    text: "This app combines a slew limiter with offset and attenuverter functions. Input and output range is configurable as either ±5V (default) or 0–10V via the Range parameter. Jack 1 is the input, Jack 2 is the output. Color can be set in the configurator. Main functions include Fader 1 for attack and Fader 2 for decay. Shift + Fader 1 sets offset, Shift + Fader 2 sets attenuvertion. Button 1 kills the offset, Button 2 sets the attenuvertion.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V or 0–10V signals (set by Range)",
        faderTitle: "Attack",
        faderDescription: "Sets the attack time of the slew limiter",
        faderPlusShiftTitle: "Offset",
        faderPlusShiftDescription: "Applies a DC offset to the input signal",
        fnTitle: "Kill Offset",
        fnDescription: "Button 1 disables the offset",
        ledTop: "Positive input",
        ledBottom: "Negative input",
      },
      {
        jackTitle: "Output",
        jackDescription: "Outputs ±5V or 0–10V signals (set by Range)",
        faderTitle: "Decay",
        faderDescription: "Sets the decay time of the slew limiter",
        faderPlusShiftTitle: "Attenuverter",
        faderPlusShiftDescription:
          "Scales and inverts the input signal (max gain 2x)",
        fnTitle: "Set Attenuverter",
        fnDescription: "Button 2 enables or configures the attenuvertion",
        ledTop: "Positive output",
        ledBottom: "Negative output",
      },
    ],
  },
  {
    appId: 13,
    title: "Envelope Follower",
    description: "Audio amplitude to CV",
    color: "Pink",
    icon: "env-follower",
    params: ["Color", "Range"],
    storage: ["Attack", "Attenuvertion", "Offset", "Input Gain", "Muted"],
    text: "This app is an envelope follower with input and output ranges of ±5V. Jack 1 is the input, Jack 2 is the output. It includes offset and attenuverter functionality, making it ideal for driving VCAs or implementing sidechain compression. The attenuverter has a maximum gain of 2x. Main functions include Fader 1 for attack and Fader 2 for decay. Shift + Fader 1 sets offset, Shift + Fader 2 sets attenuvertion. Button 2 (no shift) mutes the output. Shift + Button 1 resets the offset to neutral, Shift + Button 2 resets the attenuation to unity. Button 1 + Fader 1 adjusts input gain from 1x to 3x.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V signals",
        faderTitle: "Attack",
        faderDescription: "Sets the attack time of the envelope follower",
        faderPlusShiftTitle: "Offset",
        faderPlusShiftDescription: "Applies a DC offset to the input signal",
        faderPlusFnTitle: "Input Gain",
        faderPlusFnDescription:
          "Adjusts input gain from 1x to 3x using Button 1 + Fader 1",
        fnTitle: "",
        fnDescription: "",
        fnPlusShiftTitle: "Reset Offset",
        fnPlusShiftDescription: "Shift + Button 1 resets offset to neutral",
        ledTop: "Positive input",
        ledBottom: "Negative input",
      },
      {
        jackTitle: "Output",
        jackDescription: "Outputs ±5V envelope signal",
        faderTitle: "Decay",
        faderDescription: "Sets the decay time of the envelope follower",
        faderPlusShiftTitle: "Attenuverter",
        faderPlusShiftDescription:
          "Scales and inverts the envelope signal (max gain 2x)",
        fnTitle: "Mute",
        fnDescription: "Short press (no shift) mutes/unmutes the output",
        fnPlusShiftTitle: "Reset Attenuation",
        fnPlusShiftDescription: "Shift + Button 2 resets attenuation to unity",
        ledTop: "Positive output",
        ledBottom: "Negative output",
      },
    ],
  },
  {
    appId: 14,
    title: "Quantizer",
    description: "Quantize CV passing through",
    color: "Blue",
    icon: "quantize",
    params: ["Color", "Range"],
    storage: ["Octave shift", "Semitone shift", "Offset toggles"],
    text: "This app is a simple quantizer that processes CV signals. The input and output range is configurable as either ±5V (default) or 0–10V via the Range parameter. Jack 1 is the input, Jack 2 is the output. The quantizer applies pitch quantization to the incoming CV. Fader 1 performs semitone shifts (0–12 semitones), and Fader 2 performs octave shifts (±5 octaves). These shifts are applied before quantization. Button 1 toggles semitone shift, and Button 2 toggles octave shift.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V or 0–10V CV signals (set by Range)",
        faderTitle: "Semitone Shift",
        faderDescription: "Shifts the CV by 0–12 semitones before quantization",
        fnTitle: "Toggle Semitone Shift",
        fnDescription: "Enables/disables semitone shift",
        ledTop: "Displays semitone level",
        ledBottom: "",
      },
      {
        jackTitle: "Output",
        jackDescription:
          "Outputs quantized ±5V or 0–10V CV signals (set by Range)",
        faderTitle: "Octave Shift",
        faderDescription: "Shifts the CV by ±5 octaves before quantization",
        fnTitle: "Toggle Octave Shift",
        fnDescription: "Enables/disables octave shift",
        ledTop: "Positive output",
        ledBottom: "Negative output",
      },
    ],
  },

  {
    appId: 15,
    title: "MIDI to CV",
    description: "Multifunctional MIDI to CV",
    color: "Cyan",
    icon: "knob-round",
    params: [
      "Mode",
      "Curve",
      "MIDI Channel",
      "MIDI CC",
      "Bend Range",
      "Note",
      "Color",
      "Velocity on Gate",
    ],
    storage: ["Attenuation", "Muted"],
    text: "This app converts MIDI messages into CV signals. It supports multiple modes, each with different output behaviors. The output range is typically 0–10V, except for Pitch Bend mode which uses ±5V. When the `Velocity on Gate` toggle is activated the gate voltage in `Gate` and `Note Gate` modes is directly related to the velocity of the MIDI note with the minimum velocity being 1V and maximum 10V. Parameters include MIDI channel, curve shaping (for CC and Aftertouch), pitch bend range. The Note Gate mode is especially useful for triggering drum modules, as it allows individual gate outputs to be assigned to specific MIDI notes—ideal for drum sequencing setups.",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "0–10V (+/- 5V in Pitch bend mode)",
        faderTitle: "Offset",
        faderDescription:
          "Offset in CC and Aftertouch mode, Octave shift in V/oct mode",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription:
          "Attenuates the CV input signal in CC and Aftertouch mode",
        fnTitle: "Mute",
        fnDescription: "Mutes the output",
        ledTop: "Positive level",
        ledBottom: "Negative level",
      },
    ],
  },

  {
    appId: 16,
    title: "CV2MIDI",
    description: "CV to MIDI CC",
    color: "Violet",
    icon: "note-grid",
    params: ["Range", "MIDI Channel", "MIDI CC", "Color", "NRPN"],
    storage: ["Attenuation", "Muted", "Offset"],
    text: "This app converts CV signals into MIDI CC messages. Jack 1 is the input. The configurator allows setting the input mode (unipolar or bipolar), MIDI channel, and MIDI CC. Main functions include Fader 1 for offset adjustment and Shift + Fader 1 for CV input attenuation. Button 1 mutes the output. All parameters are stored in scenes.",
    channels: [
      {
        jackTitle: "CV Input",
        jackDescription:
          "Accepts CV signals (±5V or 0–10V depending on configuration)",
        faderTitle: "Offset",
        faderDescription: "Adjusts the offset of the incoming CV signal",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Attenuates the CV input signal",
        fnTitle: "Mute",
        fnDescription: "Button 1 mutes the MIDI output",
        ledTop: "Positive level",
        ledBottom: "Negative level",
      },
    ],
  },

  {
    appId: 17,
    title: "CV/OCT to MIDI",
    description: "CV and gate to MIDI note converter",
    color: "Orange",
    icon: "note-box",
    params: ["Range", "MIDI Channel", "Delay (ms)", "Color"],
    storage: [
      "Octave shift",
      "Semitone shift",
      "Muted",
      "Semitone offset toggle",
    ],
    text: "This app converts V/oct and gate signals into MIDI notes. Jack 1 is the V/oct input, and Jack 2 is the gate input. The input CV can be bipolar. The configurator allows setting the MIDI channel and delay compensation. MIDI CC is currently unused and will be removed. The delay parameter is useful when the CV signal arrives slightly after the gate. Main functions include Fader 1 for semitone shift (0–12 st) and Fader 2 for octave shift (±5 octaves). Button 1 toggles semitone offset, and Button 2 mutes the MIDI output.",
    channels: [
      {
        jackTitle: "V/oct Input",
        jackDescription: "Accepts pitch CV (±5V)",
        faderTitle: "Semitone Shift",
        faderDescription:
          "Shifts pitch CV by 0–12 semitones before MIDI conversion",
        fnTitle: "Toggle Semitone Offset",
        fnDescription: "Button 1 enables/disables semitone offset",
        ledTop: "Pitch CV activity",
        ledBottom: "Pitch CV activity",
      },
      {
        jackTitle: "Gate Input",
        jackDescription: "Triggers MIDI note-on events",
        faderTitle: "Octave Shift",
        faderDescription:
          "Shifts pitch CV by ±5 octaves before MIDI conversion",
        fnTitle: "Mute",
        fnDescription: "Button 2 mutes MIDI output",
        ledTop: "Gate activity",
        ledBottom: "",
      },
    ],
  },
  {
    appId: 18,
    title: "Clock Divider",
    description: "Simple clock divider",
    color: "Orange",
    icon: "note-box",
    params: ["MIDI Channel", "MIDI Note", "GATE %", "Divisions", "Color"],
    storage: ["Division", "Muted", "Maximum division", "Minimum division"],
    text: "This is a simple clock divider app that was suggested by youtuber and Discord member Synthdad. The app allows for a performative control of clock division/multiplication allowing for 'build ups and drops' for example. The maximum and minimum divisions can be user set using shift + fader and button + fader respectively. These are saved into the scenes allowing you to set different ranges depending on your needs. Button (no shift) mutes the output. The **Divisions** parameter selects which divider set is available to the fader: **Straight**, **Triplets**, or **Both**.",
    channels: [
      {
        jackTitle: "Trigger out",
        jackDescription: "Sends triggers on clock",
        faderTitle: "Division",
        faderDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar, 2 bars, 4 bars",
        faderPlusFnTitle: "Minimum division",
        faderPlusShiftTitle: "Maximum division",
        fnTitle: "Mute",
        fnDescription: "",
        fnPlusShiftTitle: "",
        ledTop: "Trigger activity indicator",
        ledTopPlusShift: "Maximum division (orange: triplet, blue: straight)",
        ledBottomPlusShift:
          "Minimum division (orange: triplet, blue: straight)",
        ledBottom: "",
      },
    ],
  },
  {
    appId: 19,
    title: "Panner",
    description:
      "Use with 2 VCA to do panning or cross fading with internal LFO for modulation",
    color: "Blue",
    icon: "stereo",
    params: [
      "Curve",
      "Range",
      "MIDI Channel",
      "MIDI CC 1",
      "MIDI CC 2",
      "Mute on release",
      "Color",
      "Store state",
      "NRPN",
    ],
    storage: [
      "Level (if 'Store state' enabled)",
      "Muted (if 'Store state' enabled)",
      "Attenuation",
      "Pan value",
      "LFO speed",
      "LFO amount",
      "LFO waveform",
    ],
    text: "This app uses two slots and is designed to control two VCAs for panning or crossfading. Fader 1 adjusts overall volume, while Fader 2 sets the pan or crossfade position. Button 1 functions as a mute. The maximum voltage range can be set in the parameters, and the output range can be fine-tuned using the internal attenuator via Shift + Fader 1. The selected range’s bipolarity also determines the CV and CC values when muted: in the 0V to 10V range, mute corresponds to 0V and CC 0—ideal for volume or send level control—while in the -5V to 5V range, mute is at 0V and CC 64, making it better suited for panning or crossfading. An internal LFO enables autopanning or auto crossfading by modulating the pan level set by Fader 2. The modulation amount is controlled with Shift + Fader 2, LFO speed with Button 2 + Fader 2, and the waveform is selected using Shift + Button 2.",
    channels: [
      {
        jackTitle: "Out 1",
        jackDescription: "CV output for VCA 1",
        faderTitle: "Volume",
        faderDescription: "Controls overall output level",
        faderPlusShiftTitle: "Attenuation level",
        faderPlusShiftDescription: "Reduces the CV and CC range",
        fnTitle: "Mute",
        fnDescription: "",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
      },
      {
        jackTitle: "Out 1",
        jackDescription: "CV output for VCA 1",
        faderTitle: "Volume",
        faderDescription: "Controls overall output level",
        faderPlusShiftTitle: "LFO amount",
        faderPlusShiftDescription: "Add LFO modulation to the pan",
        faderPlusFnTitle: "LFO Speed",
        fnTitle: "None",
        fnDescription: "",
        fnPlusShiftTitle: "LFO Waveform selection",
        fnPlusShiftDescription:
          "Sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), and square (white)",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
      },
    ],
  },
  {
    appId: 20,
    title: "Random+",
    description: "Random CC/CV with assignable CV input",
    color: "Green",
    icon: "random",
    params: [
      "Bipolar",
      "MIDI Channel",
      "MIDI CC",
      "Send MIDI",
      "Color",
      "NRPN",
    ],
    storage: [
      "Speed",
      "Muted",
      "Attenuation",
      "Slew",
      "Clocked",
      "Input attenuation",
      "Input mute",
      "Input destination",
    ],
    text: `Random+ extends Random CC/CV with an assignable CV input lane for real-time modulation. Channel 1 handles CV input: Fader 1 sets attenuation and Button 1 mutes/unmutes the lane. Use **Shift + Button 1** to choose CV destination:

- speed (yellow)
- ext clock (pink)
- slew (cyan)
- sample & hold (white)

In speed mode, incoming CV offsets the speed setting. In free-running mode this changes the internal interval, and in clocked mode it offsets the selected timing-resolution index. In ext clock mode, new random values are generated only when a rising edge is detected (around 1V after attenuation/mute processing). In slew mode, incoming CV modulates transition smoothing. In sample & hold mode, the random generator is bypassed: on each trigger (same clock/free-run timing as the other modes), the attenuated CV input is sampled and held as the output value instead of a new random roll, turning Random+ into a clocked sample-and-hold.

Channel 2 is the random output lane: Fader 2 sets base speed, **Shift + Fader 2** sets attenuation, and **Button 2 + Fader 2** sets slew. Use **Shift + short press** on Button 2 to mute/unmute output, and **Shift + long press** to toggle free-running versus clocked mode.

Output range can be unipolar (0–10V) or bipolar (-5V to +5V), and MIDI CC follows the same random output stream.`,
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "-5V to 5V CV in",
        faderTitle: "CV attenuation",
        faderDescription: "Attenuates the incoming CV",
        faderPlusShiftTitle: "",
        faderPlusShiftDescription: "",
        fnTitle: "CV input mute",
        fnDescription: "Mutes/unmutes the CV lane",
        fnPlusShiftTitle: "CV destination",
        fnPlusShiftDescription:
          "Speed (yellow), ext clock (pink), slew (cyan), sample & hold (white)",
        ledTop: "Positive input level indicator",
        ledTopPlusShift: "Destination color on button",
        ledBottom: "Negative input level indicator",
      },
      {
        jackTitle: "Output",
        jackDescription: "Random CV out (0–10V or -5V to +5V)",
        faderTitle: "Speed",
        faderDescription: "Sets base random speed",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces output range",
        faderPlusFnTitle: "Slew",
        faderPlusFnDescription: "Sets random transition smoothing",
        fnTitle: "No direct action",
        fnDescription: "",
        fnPlusShiftTitle: "Mute / Clock mode",
        fnPlusShiftDescription: "Short: mute, Long: toggle free/clocked",
        ledTop: "Positive output level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative output level indicator",
      },
    ],
  },
  {
    appId: 21,
    title: "Clock Divider+",
    description: "Clock divider with assignable CV input",
    color: "Orange",
    icon: "note-box",
    params: ["MIDI Channel", "MIDI Note", "GATE %", "Divisions", "Color"],
    storage: [
      "Division",
      "Muted",
      "Maximum division",
      "Minimum division",
      "CV attenuation",
      "CV mute",
      "CV destination",
    ],
    text: "**Clock Divider+** is an extension of Clock Divider that uses two channels: one CV input channel and one trigger output channel. The output channel behaves like the original divider, including MIDI note output and gate length control, while the input channel can be assigned to different jobs.\n\nThe **Divisions** parameter lets you choose the available divider set: **Straight**, **Triplets**, or **Both**. This affects what the main fader and CV offset can select.\n\nIn normal operation, **CV destination = Division** and the input CV offsets the current divider setting around the main fader value. This makes it easy to push the rhythm denser or sparser from modulation without losing your base timing.\n\nWith **CV destination = External clock**, incoming CV rising edges (around 1V threshold) are used as the clock source, similar to the external clock mode in Random+. In this mode, the divider no longer follows the internal/global tick stream and instead counts external pulses.\n\nControls follow the plus-app layout: channel 1 handles input attenuation and input mute, channel 2 handles divider range and output mute (short press, no shift). Shift + Button 1 cycles CV destination, shown by color (division: yellow, external clock: pink).",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "-5V to 5V CV in",
        faderTitle: "CV attenuation",
        faderDescription:
          "Attenuates incoming CV before destination processing",
        faderPlusShiftTitle: "",
        faderPlusShiftDescription: "",
        fnTitle: "CV input mute",
        fnDescription: "",
        fnPlusShiftTitle: "CV destination",
        fnPlusShiftDescription: "Division (yellow), External clock (pink)",
        ledTop: "Positive input level indicator",
        ledTopPlusShift: "",
        ledBottom: "Negative input level indicator",
      },
      {
        jackTitle: "Trigger out",
        jackDescription:
          "Sends clock-divided triggers and optional MIDI note events",
        faderTitle: "Division",
        faderDescription: "Sets divider amount within the active min/max range",
        faderPlusShiftTitle: "Maximum division",
        faderPlusShiftDescription: "Sets the upper divider limit",
        faderPlusFnTitle: "Minimum division",
        faderPlusFnDescription: "Sets the lower divider limit",
        fnTitle: "Mute",
        fnDescription: "",
        fnPlusShiftTitle: "",
        fnPlusShiftDescription: "",
        ledTop: "Trigger activity indicator",
        ledTopPlusShift: "Maximum division indicator",
        ledBottom: "",
        ledBottomPlusShift: "Minimum division indicator",
      },
    ],
  },
  {
    appId: 22,
    title: "LFO+",
    description: "Multi shape LFO",
    color: "Yellow",
    icon: "sine",
    params: [
      "Speed division",
      "Range",
      "MIDI Channel",
      "MIDI CC",
      "Color",
      "NRPN",
      "Send MIDI",
      "Grid Lock",
    ],
    storage: [
      "CV attenuation",
      "CV mute",
      "CV destination",
      "Clocked",
      "Attenuation",
      "Speed",
      "Waveform",
      "Output Muted",
    ],
    text: `LFO+ extends the standard LFO with an assignable CV input on the first channel.

#### CV input

The fader attenuates the incoming CV; the button mutes it. Use **Shift + Button 1** to cycle through CV destinations, shown by the button color:

- **Speed** (yellow) — modulates LFO rate through zero, so the waveform inverts and speeds up again with negative CV
- **Phase** (pink) — modulates the LFO phase directly
- **Amplitude** (cyan) — modulates output attenuation
- **Reset** (red) — resets the LFO phase on a rising edge above 1V; attenuation and mute state affect the detection threshold

#### Waveform and output

Press **Button 2** to cycle through waveforms: sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), square (white). **Shift + Fader 2** adjusts output attenuation. **Long press** (no shift) on Button 2 mutes the LFO output.

#### Clocked mode

**Shift + long press** on Button 2 toggles between free-running and tempo-synced modes. In clocked mode, available resolutions are: 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, and bar. The **Speed** parameter applies a global multiplier—Normal, Slow (÷2), Slowest (÷4)—in both modes. Output can be bipolar (−5V to +5V) or unipolar (0V to 10V), setting the MIDI CC center to 64 or 0 respectively.

#### Grid Lock

**Grid Lock only has an effect in clocked mode.** When enabled (default on), the LFO phase is continuously derived from the clock's absolute tick count, keeping it locked to the grid regardless of when the LFO was started. Changing the speed division re-aligns automatically, but this can cause a click as the LFO jumps to its recalculated position.

A **Shift + short press** on Button 2, or a rising-edge gate when the CV input is in reset mode, offsets the phase reference to the current tick for deliberate phase-offset effects. A clock reset re-locks to the grid.

Disabling Grid Lock reverts to free-running phase accumulation: the LFO will smoothly speed up or slow down from its current position when the division changes, with no jump.`,
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "-5V to 5V CV in",
        faderTitle: "CV attenuation",
        faderDescription: "Attenuates the incoming CV",
        faderPlusShiftTitle: "",
        faderPlusShiftDescription: "",
        fnTitle: "CV input Mute",
        fnDescription: "",
        fnPlusShiftTitle: "CV destination",
        fnPlusShiftDescription:
          "Speed (yellow), phase (pink), amplitude (cyan), reset (red)",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "",
        ledBottom: "Negative level indicator",
      },
      {
        jackTitle: "Output",
        jackDescription: "-5V to 5V LFO out",
        faderTitle: "LFO speed",
        faderDescription:
          "Sets the LFO speed, top is maximum and bottom slowest",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range",
        fnTitle: "Waveform / Mute",
        fnDescription: "Short: cycle waveform. Long (no shift): mute output",
        fnPlusShiftTitle: "Reset / Clocked mode",
        fnPlusShiftDescription: "Short: reset LFO. Long: toggle clocked mode",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
      },
    ],
  },
  {
    appId: 23,
    title: "FP-Grids",
    description:
      "Emilie Gillet's renowned Mutable Instruments Grids topographic drum sequencer for the ATOV Faderpunk, with an extra Drum n' Bass mode",
    color: "Orange",
    icon: "euclid",
    params: [
      "MIDI mode",
      "Note 1 MIDI channel",
      "MIDI Note 1",
      "Note 2 MIDI Channel",
      "MIDI Note 2",
      "Note 3 MIDI Channel",
      "MIDI Note 3",
      "DnB Ghost Note MIDI Channel",
      "DnB Ghost Note MIDI Note",
      "MIDI Velocity",
      "MIDI velocity (Accent)",
      "Gate %",
      "Color",
    ],
    storage: [
      "Output Mode",
      "Drums Density",
      "Drums Map X & Y",
      "Euclidean Fill",
      "Euclidean Length",
      "Euclidean Offset",
      "Chaos",
      "Division",
      "Trigger Mutes",
      "DnB Pattern",
    ],
    text: `
Grids is described as a "topographic drum sequencer" - it generates a variety of drum patterns based on continuous interpolation through a "map" of patterns (Drum Mode) or using Euclidean algorithms (Euclidean Mode).  The original Mutable Instruments module manual is [here](https://pichenettes.github.io/mutable-instruments-documentation/modules/grids/manual/).

* FP-Grids outputs CV gates (0V = off, 10V = on) and, optionally MIDI note on/off messages, with normal and accented velocity levels.

#### Drums Output Mode

Generates patterns by interpolating through a 2D map of pre-analyzed drum patterns. Sequence length is always 32 steps at 1/32nd note resolution.

* **Map X / Map Y:** Controls the position on the pattern map. Small changes typically result in related rhythmic variations.
* **Density 1 / Density 2 / Density 3:** Controls the event density (fill) for each of the three main trigger outputs.
* **Chaos Amount:** Controls the amount of randomness applied. When set to a high value, rolls / ghost notes will be randomly added to the pattern.
* **Global Accent:** The 4th channel provides a global accent CV gate that can be used for triggering other voices or envelopes. This combines the accents from the individual three drum voices in the original Grids firmware into a single mixed Accent signal.

#### Euclidean Output Mode

Generates classic Euclidean rhythms for each of the three main trigger outputs independently.

* **Length 1 / Length 2 / Length 3:** Sets the total number of steps in the sequence for each output (1-16). Set with Shift + Fader.
* **Fill 1 / Fill 2 / Fill 3:** Main faders set the number of active beats from 0 to the current Length for each channel.
* **Offset 1 / Offset 2 / Offset 3:** While holding Shift, press channel buttons 1-3 to rotate each Euclidean pattern by one step.
* **Chaos Amount:** Controls the probability of randomly flipping a beat on or off each step.
* **Clock Division:** Shift + Fader 4 sets the step resolution (default 1/16th note).
* While Shift is held in Euclidean mode, channel buttons light pink to indicate offset control.

#### DnB Mode (Easter Egg)

A drum and bass pattern generator with 12 preset kick/snare/hi-hat patterns and probabilistic triggering. Cycle through output modes with Shift + Button 4 until the LED shows sand/amber.

* **Outputs:** Kick, Snare, Hats and Ghost Snare CV gates and MIDI notes
* **Kick / Snare / Ghost Snare:** Faders 1, 2, and 4 set the trigger probability for each voice (0 = never, full = always).
* **Pattern select:** Fader 3 selects one of 12 DnB patterns. The pattern changes take effect at the start of the next bar.
* **Vary pattern:** Shift + Button 1 randomly mutates the current pattern.
* **Restore pattern:** Shift + Button 2 restores the pattern to the last selected base pattern.
* Clock division is set automatically by the selected pattern — Fader 4 Alt (resolution) has no effect in this mode.
* The Ghost Snare uses the the "DnB Ghost Note MIDI Note" and MIDI Channel, at a reduced velocity.

#### Patch Ideas

* Try saving different Scenes with different Output Modes, then switching between scenes in a performance (sequence will reset on next step).
* The sequencers can be reset rhythmically by patching an external trigger into one of the Faderpunk Aux Jacks (configured as a reset input).

#### Acknowledgements

* Original Concept & Code: Emilie Gillet (Mutable Instruments). The original Eurorack module source code can be found [here](https://github.com/pichenettes/eurorack/tree/master/grids).
* Faderpunk Port: Richard Smith (Discord: phommed)
* Special acknowledgement: [Disting NT "nt_grids" Port](https://github.com/thorinside/nt_grids/tree/main) by Neal Sanche (GitHub: Thorinside)

#### Channels

Fader functions vary by output mode. Drums / Euclidean / DnB descriptions are shown where they differ.

`,
    channels: [
      {
        jackTitle: "Trigger output 1",
        jackDescription: "Bass drum / Euclidean Ch1 / Kick gate output",
        faderTitle: "Density 1 / Fill 1 / Kick Probability",
        faderDescription:
          "Drums: note density for trigger 1. Euclidean: fill amount (0 to current length). DnB: kick trigger probability.",
        faderPlusShiftTitle: "Map X / Euclidean Length 1",
        faderPlusShiftDescription:
          "Drums: interpolating scan through drum map X axis. Euclidean: pattern length 1-16 steps. DnB: no function.",
        ledTop: "Gate output 1",
        ledBottom: "Density / Fill / Probability level",
        ledBottomPlusShift: "Map X amount",
        fnTitle: "Mute 1",
        fnDescription: "Mute trigger 1",
        fnPlusShiftTitle: "Euclidean Offset 1 / Vary DnB Pattern",
        fnPlusShiftDescription:
          "Euclidean: Shift + Button 1 rotates pattern 1 by one step. DnB: Shift + Button 1 randomly varies the current pattern.",
      },
      {
        jackTitle: "Trigger output 2",
        jackDescription: "Snare / Euclidean Ch2 / Snare gate output",
        faderTitle: "Density 2 / Fill 2 / Snare Probability",
        faderDescription:
          "Drums: note density for trigger 2. Euclidean: fill amount (0 to current length). DnB: snare trigger probability.",
        faderPlusShiftTitle: "Map Y / Euclidean Length 2",
        faderPlusShiftDescription:
          "Drums: interpolating scan through drum map Y axis. Euclidean: pattern length 1-16 steps. DnB: no function.",
        ledTop: "Gate output 2",
        ledBottom: "Density / Fill / Probability level",
        ledBottomPlusShift: "Map Y amount",
        fnTitle: "Mute 2",
        fnDescription: "Mute trigger 2",
        fnPlusShiftTitle: "Euclidean Offset 2 / Restore DnB Pattern",
        fnPlusShiftDescription:
          "Euclidean: Shift + Button 2 rotates pattern 2 by one step. DnB: Shift + Button 2 restores the pattern to the base.",
      },
      {
        jackTitle: "Trigger output 3",
        jackDescription: "Hi-Hat / Euclidean Ch3 / Hi-Hat gate output",
        faderTitle: "Density 3 / Fill 3 / DnB Pattern Select",
        faderDescription:
          "Drums: note density for trigger 3. Euclidean: fill amount (0 to current length). DnB: selects one of 12 patterns (change takes effect at next bar).",
        faderPlusShiftTitle: "Euclidean Length 3",
        faderPlusShiftDescription:
          "Euclidean: pattern length 1-16 steps. Drums / DnB: no function.",
        ledTop: "Gate output 3",
        ledBottom: "Density / Fill / Pattern number",
        fnTitle: "Mute 3",
        fnDescription: "Mute trigger 3",
        fnPlusShiftTitle: "Euclidean Offset 3",
        fnPlusShiftDescription:
          "Euclidean: Shift + Button 3 rotates pattern 3 by one step. Drums / DnB: no function.",
      },
      {
        jackTitle: "Accent / Accent / Ghost Snare gate output",
        jackDescription:
          "Global accent gate output (Drums / Euclidean) or Ghost Snare gate output (DnB)",
        faderTitle: "Chaos / Ghost Probability",
        faderDescription:
          "Drums / Euclidean: pattern randomisation and humanisation. DnB: ghost snare trigger probability.",
        faderPlusShiftTitle: "Resolution",
        faderPlusShiftDescription:
          "Sets clock resolution (Drums / Euclidean only): 32ndT, 32nd, 16thT, 16th (default), 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar. No effect in DnB mode.",
        ledTop: "Accent / Ghost Snare gate output",
        ledBottom: "Chaos / Ghost Probability level",
        ledBottomPlusShift: "Resolution in blue, 16th note shown in yellow",
        fnTitle: "Accent / Ghost Mute",
        fnDescription: "Mute accent (Drums / Euclidean) or ghost snare (DnB)",
        fnPlusShiftTitle: "Cycle Output Mode",
        fnPlusShiftDescription:
          "Cycles through output modes: Light Blue = Drums, Pink = Euclidean, Sand = DnB",
      },
    ],
  },
  {
    appId: 24,
    title: "TB-3PO",
    description: "TB-303 acid pattern generator",
    color: "Orange",
    icon: "softRandom",
    params: ["MIDI Channel", "MIDI Out", "Color", "1V/Oct"],
    storage: [
      "Seed (pattern identity)",
      "Density",
      "Sequence length",
      "Transpose (semitones)",
      "Octave transpose",
      "Clock resolution",
      "Muted",
      "No accents",
    ],
    text: `
TB-3PO is a deterministic acid bass pattern generator, ported from the TB-3PO Hemisphere applet by Logarhythm/djphazer. It generates TB-303-style patterns — complete with gates, accents, and octave shifts — from a single 16-bit seed value. Because patterns are fully deterministic, the same seed always produces the same sequence, making it easy to recall and lock in a favourite groove.

#### Pattern Generation

Every pattern is generated in two passes from the current seed and density:

1. **Pitch pass:** assigns scale degrees (0–8) and octave-up/down flags to each of 32 steps. Higher density means more pitch variety and fewer repeated notes.
2. **Gate/accent pass:** rolls gate and accent probability for each step. Accents cluster similarly to a real 303.

The quantizer maps all pitch output to the system-wide scale and root, so TB-3PO stays in key across your whole patch.

#### Outputs

* **Ch 1 (Jack 0):** Pitch CV (0–10V, 1V/oct)
* **Ch 2 (Jack 1):** Gate
* **Ch 3 (Jack 2):** Accent CV (high when accented, 0 otherwise)

#### Faders

* **Fader 1 (Density):** Controls pattern density and pitch variety simultaneously. At low values the pattern is sparse and monotone; at high values it is dense and chromatically varied. While Button 1 is held, Fader 1 selects the clock resolution (whole note down to fast 32nds).
* **Fader 2 (Length):** Sets the sequence length from 1 to 32 steps.
* **Fader 3 (Transpose):** Transposes the entire pattern ±24 semitones. Hold Shift while moving Fader 3 to shift by whole octaves (−4 to +4). Both offsets are summed and stored independently.

#### Buttons

* **Button 1 — short press:** Re-seeds the pattern. A new seed is grabbed from the internal tick counter, immediately generating a fresh pattern and resetting the step counter.
* **Button 2 — short press:** Toggles accents on/off. Button lit mid = accents active; button dim = accents suppressed (accent CV stays 0, MIDI fires at normal velocity).
* **Button 3 — short press:** Mutes or unmutes the output. Mute is inhibit-only: the current note rings out naturally (gate closes at end of the step), pitch CV holds its last value, and no new gates open until unmuted.

#### Clock & Re-seeding

On a clock **Reset**, the step counter resets to step 1 — the pattern is not changed. On a clock **Stop**, the gate closes and any held MIDI note is killed.

#### LED Feedback

* **Ch 1 Top:** Density level as brightness (user color)
* **Ch 1 Bottom:** While Button 1 is held (resolution mode), flashes in sync with the current clock division — orange for straight divisions, blue for triplets.
* **Ch 1 Button:** Mid brightness (user color); flashes white on each reseed.
* **Ch 2 Top:** Gate open indicator (user color)
* **Ch 2 Bottom:** Step progress — bright at step 1, dims toward the end of the sequence
* **Ch 2 Button:** Mid brightness = accents active; dim = accents suppressed
* **Ch 3 Top:** Orange when an accented gate is firing
* **Ch 3 Bottom:** Transpose distance from center — dim = centered (no offset), bright = far from center
* **Ch 3 Button:** Lit = unmuted; dim = muted

#### Acknowledgements

* Original concept & code: Logarhythm / djphazer ([TB-3PO Hemisphere applet](https://github.com/djphazer/O_C-BenisphereSuite))
`,
    channels: [
      {
        jackTitle: "Pitch CV",
        jackDescription: "0–10V quantized pitch output (1V/oct)",
        faderTitle: "Density",
        faderDescription:
          "Controls gate density and pitch variety. Low = sparse/monotone, high = dense/chromatic.",
        faderPlusFnTitle: "Clock resolution",
        faderPlusFnDescription:
          "Hold Button 1 to select clock resolution: whole note → fast 32nds (8 steps, orange = straight, blue = triplet)",
        fnTitle: "Re-seed",
        fnDescription: "Short press: generate a new random pattern.",
        ledTop: "Density level",
        ledBottom: "In resolution mode: division flash.",
      },
      {
        jackTitle: "Gate",
        jackDescription: "Gate output",
        faderTitle: "Sequence length",
        faderDescription: "Sets the number of active steps (1–32)",
        fnTitle: "Toggle accents",
        fnDescription: "Short press toggles accents on/off.",
        ledTop: "Gate open (user color)",
        ledBottom:
          "Step progress — bright at step 1, dims toward end of sequence",
      },
      {
        jackTitle: "Accent CV",
        jackDescription:
          "High when the current gated step is accented, 0 otherwise",
        faderTitle: "Transpose",
        faderDescription: "Transposes the pattern ±24 semitones",
        faderPlusFnTitle: "Octave transpose",
        faderPlusFnDescription:
          "Hold Shift while moving Fader 3 to transpose by whole octaves (−4 to +4). Both offsets are summed.",
        fnTitle: "Mute",
        fnDescription:
          "Short press mutes/unmutes. Inhibit-only: current note rings out, pitch CV holds, no new gates until unmuted.",
        ledTop: "Orange when an accented gate is firing",
        ledBottom:
          "Transpose distance from center — dim = no offset, bright = far from center.",
      },
    ],
  },
  {
    appId: 25,
    title: "Automator",
    description: "CV gesture looper",
    color: "Cyan",
    icon: "fader",
    params: [
      "MIDI Channel",
      "MIDI CC",
      "Range",
      "Color",
      "NRPN",
      "MIDI Out",
      "Resolution",
    ],
    storage: [
      "Committed loop buffer",
      "Loop length",
      "Attenuator level",
      "Offset level",
    ],
    text: `The Automator is an automation recorder. To record a loop hold the button while moving the fader and release to lock it in as a repeating loop. After recording the loop the fader becomes an offset to the output CV. Like in most apps shift + fader is an attenuator, in this case the attenuation only affects the recorded CV allowing you to introduce this modulation gradually.

**A clock is required.** Without a clock the fader passes through to the output but recording is disabled and the button does nothing.

#### Recording a loop

Hold the button and move the fader. The LED turns red while you hold. Both the recording start and end are quantized to the nearest 16th-note, the loop length equals the button hold duration. If the maximum recording length is reached, the recording stops automatically and the loop starts playing. Hold the button again to replace the loop. Press Shift + Button to clear the loop and return to passthrough.

#### Loop controls

While a loop is running, the fader adds a bipolar offset to the loop. Center leaves the loop unmodified; pushing up raises the output, pulling down lowers it. Hold Shift and move the fader to set the attenuation level, in this case the attenuation only affects the recorded CV allowing you to introduce this modulation gradually. The button LED turns white for one tick at the start of each loop cycle. Both offset and attenuation are saved with the loop.

#### Persistence

The loop, its length, offset, and attenuation are saved to memory and restored on power-up. Loops are saved and recalled per scene.

#### Resolution

The resolution parameter controls how many samples are recorded per bar. Higher values give finer resolution but shorten the maximum loop length. Lower values allow longer loops at the cost of playback resolution. The CV is interpolated between samples`,
    channels: [
      {
        jackTitle: "CV Output",
        jackDescription:
          "CV output — fader direct in passthrough, loop playback when playing",
        faderTitle: "CV level (passthrough) / Loop offset (playing)",
        faderDescription:
          "In passthrough: controls CV output directly. While a loop is playing: offsets the loop output up or down from center.",
        faderPlusShiftTitle: "Loop attenuator",
        faderPlusShiftDescription:
          "While a loop is playing: reduces the output range. Has no effect in passthrough.",
        fnTitle: "Hold to record",
        fnDescription:
          "Hold to capture a gesture, release to commit it as a loop. Commits automatically if the maximum length is reached. Hold again to re-record. Button turns red while recording, then flashes white for one clock tick at the start of each loop cycle.",
        fnPlusShiftTitle: "Clear loop",
        fnPlusShiftDescription:
          "Clears the loop and returns to passthrough. Works from any state.",
        ledTop:
          "Output level — app color in passthrough, red while recording, green while playing",
        ledBottom: "Output level (bipolar range only)",
      },
    ],
  },
  {
    appId: 26,
    title: "GenSeq",
    description: "Generative sequencer with Turing machine registers",
    color: "Blue",
    icon: "sequence-square",
    params: [
      "MIDI Channel",
      "Base Note",
      "Color",
      "MIDI Out",
      "1V/Oct",
      "Bypass quantizer",
    ],
    storage: [
      "Pitch range",
      "Length attenuator",
      "Beat density",
      "Legato density threshold",
      "Accent density threshold",
      "Clock resolution",
      "Octave shift",
      "Gate length",
      "Pitch register (persisted)",
      "Length register",
      "Pitch TM register width (1–16)",
      "Length TM register width (1–16)",
      "Muted",
    ],
    text: `GenSeq is a generative melodic sequencer built around two Turing machine shift registers. The pitch TM determines which note plays; the length TM determines the Euclidean pattern length. Both evolve slowly each cycle, so the melody and rhythm drift together over time. Legato and accents are derived automatically from the same TMs, so the whole sequence, melody, rhythm, slides and accents, come from just two seeds.

#### Register lengths

Hold Shift and tap Button 1 or Button 2 to count out a TM length (1–16 steps); release Shift to commit. The button LED brightens with each tap and shows the stored value before you start counting. If the length of the length TM is set to 1 then the euclidean generator behaves the same as a standard one with Fader 2 controlling the length and Fader 3 the pulse count.

#### Mute

Mute is inhibit-only. Tapping Button 3 stops new gates from opening and holds the CV at its last value, but the current note rings out naturally. Holding Button 3 to access the third layer does not toggle mute.

#### Scene recall

On load, both registers are restored at the next phrase boundary so the recalled sequence re-enters in time.`,
    channels: [
      {
        jackTitle: "CV Output",
        jackDescription: "Quantized pitch, 0–10V",
        faderTitle: "Pitch range",
        faderDescription: "Spread between lowest and highest notes",
        faderPlusShiftTitle: "Octave shift",
        faderPlusShiftDescription: "−2 to +2 octaves",
        faderPlusFnTitle: "Clock resolution",
        faderPlusFnDescription: "Step rate (1/1 to 1/16)",
        fnTitle: "Mutate pitch",
        fnDescription: "Hold to mutate, release to lock",
        ledTop: "Current note height",
        ledBottom: "Clock flash (Button 3 layer)",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Gate signal, 0–10V",
        faderTitle: "Length attenuator",
        faderDescription: "Euclidean pattern length",
        faderPlusShiftTitle: "Legato density",
        faderPlusShiftDescription: "How many steps slide",
        fnTitle: "Mutate length",
        fnDescription: "Hold to mutate rhythm",
        ledTop: "Gate open",
        ledBottom: "Length TM cycle progress",
      },
      {
        jackTitle: "Accent CV Output",
        jackDescription: "10V on accented steps, 0V otherwise",
        faderTitle: "Beat density",
        faderDescription: "How many steps trigger a gate",
        faderPlusShiftTitle: "Accent density",
        faderPlusShiftDescription: "How many steps are accented",
        faderPlusFnTitle: "Gate length",
        faderPlusFnDescription: "Gate duration (1–99%)",
        fnTitle: "Toggle mute",
        fnDescription: "Tap to toggle mute",
        ledTop: "Legato slide",
        ledBottom: "Euclidean cycle progress",
      },
    ],
  },
];

export const ManualTab = () => {
  const location = useLocation();
  const navigate = useNavigate();
  useEffect(() => {
    if (location.hash) {
      const element = document.getElementById(location.hash.slice(1));
      if (element) {
        element.scrollIntoView({ behavior: "smooth" });
      }
    }
    return () => {
      if (location.hash) {
        navigate(location.pathname + location.search, { replace: true });
      }
    };
  }, [location, navigate]);
  return (
    <>
      <H2>A quick note</H2>
      <p>
        This manual is currently under heavy development. Check back regularly
        for updates.
      </p>
      <H2>Contents</H2>
      <nav>
        <List>
          <li>
            <Link to="#preface">Preface</Link>
          </li>
          <li>
            <Link to="#interface">Interface</Link>
            <List>
              <li>
                <Link to="#front-panel">Front Panel Overview</Link>
              </li>
              <li>
                <Link to="#additional-controls">Additional Controls</Link>
              </li>
              <li>
                <Link to="#global-parameters">Global Parameters Access</Link>
              </li>
              <li>
                <Link to="#back-connectors">Back Connectors</Link>
              </li>
              <li>
                <Link to="#internal-connectors">Internal Connectors</Link>
              </li>
              <li>
                <Link to="#important-points">Important Points</Link>
              </li>
            </List>
          </li>
          <li>
            <Link to="#configurator">Configurator</Link>
            <List>
              <li>
                <Link to="#compatible-browsers">Compatible Browsers</Link>
              </li>
              <li>
                <Link to="#device-tab">Device Tab</Link>
              </li>
              <li>
                <Link to="#apps-tab">Apps Tab</Link>
              </li>
              <li>
                <Link to="#settings-tab">Settings Tab</Link>
                <List>
                  <li>
                    <Link to="#settings-clock">Clock</Link>
                  </li>
                  <li>
                    <Link to="#settings-quantizer">Quantizer</Link>
                  </li>
                  <li>
                    <Link to="#settings-midi">MIDI</Link>
                  </li>
                  <li>
                    <Link to="#settings-i2c">I²C</Link>
                  </li>
                  <li>
                    <Link to="#settings-aux">AUX Jacks</Link>
                  </li>
                  <li>
                    <Link to="#settings-misc">Miscellaneous</Link>
                  </li>
                  <li>
                    <Link to="#settings-save-recall">Save & Recall Setup</Link>
                  </li>
                </List>
              </li>
            </List>
          </li>
          <li>
            <Link to="#apps">Apps</Link>
            <List>
              <li>
                <Link to="#muting-apps">Muting apps</Link>
              </li>
              {apps.map((app) => (
                <li key={app.title}>
                  <Link to={`#app-${app.appId}`}>{app.title}</Link>
                </li>
              ))}
            </List>
          </li>
          <li>
            <Link to="#update">Update guide</Link>
          </li>
          <li>
            <Link to="#troubleshooting">Troubleshooting</Link>
            <List>
              <li>
                <Link to="#connection-issues">Connection Issues</Link>
              </li>
              <li>
                <Link to="#factory-reset">Factory Reset</Link>
              </li>
            </List>
          </li>
        </List>
      </nav>
      <Preface />
      <Interface />
      <Configurator />
      <Apps apps={apps} />
      <UpdateGuide />
      <Troubleshooting />
    </>
  );
};
