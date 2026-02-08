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
    ],
    storage: [
      "Level (if 'Store state' enabled)",
      "Muted (if 'Store state' enabled)",
      "Attenuation",
    ],
    text: "This app is designed to provide a simple way to manually control any parameters using either CV or MIDI CC. The MIDI channel and CC numbers can be adjusted in the app's settings, and both MIDI and CV outputs are always active simultaneously. The range can be adjusted using Shift + Fader, which affects both CC and CV ranges. The fader controls the level of the CV or CC, and the button acts as a clickless mute. The fader level and the mute state can be saved in scenes if the 'Store state' parameter is enabled (active by default). You can then use this app as a way to save and recall CV voltage allowing for preset in a modular system for example. The curve can be adjusted in the settings; however, this only affects the CV output. Two voltage ranges are available in the settings: 0V to 10V or -5V to 5V. Note that this range also affects the level at which CV and CC are set when muting. In the 0V to 10V range, mute is at 0V and CC 0, making it ideal for controlling volume, send levels, or similar parameters. In the -5V to 5V range, mute is at 0V and CC 64, making it suitable for controlling panning, crossfading, or similar functions. The mute behavior can be set to trigger on press or on release, depending on your preference. Due to popular demand, the app's action can also be inverted—this means that when the fader is at the top, the output will be set to the minimum value, and when at the bottom, it will send the maximum CC and CV value. As with all apps where the LED color does not serve any specific function, you are free to configure it in the settings.",
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
    params: ["Speed division", "Range", "Send MIDI", "MIDI Channel", "MIDI CC"],
    storage: ["Clocked", "Attenuation", "Speed", "Waveform"],
    text: "This is a simple LFO that lets you manually select the waveform by pressing the button, with the LED color indicating the chosen shape: sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), and square (white). You can adjust the CV output range using Shift + Fader. Shift + short press resets the waveform, while Shift + long press toggles between free-running and tempo-synced modes. In free-running mode, the speed ranges from 14 Hz down to one cycle per minute. In clocked mode, available resolutions include 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, and bar. The app parameters allow you to set the overall speed—Normal, Slow (÷2), and Slowest (÷4)—which also applies to clocked speeds. When clocked, the button flashes in sync with the LFO rate. The output can be configured to be either bipolar (-5V to +5V) or unipolar (0V to 10V) and this also affect where the attenuator will center when outputting MIDI CC, 0 when unipolar and 64 when bipolar. This app can also be configured to output MIDI CC in the parameters and MIDI channel and CC are freely configurable",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "-5V to 5V LFO out",
        faderTitle: "LFO speed",
        faderDescription:
          "Sets the LFO speed, top is maximum and bottom slowest",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range",
        fnTitle: "Waveform selection",
        fnDescription:
          "Sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), and square (white)",
        fnPlusShiftTitle: "Reset - Clocked mode",
        fnPlusShiftDescription: "Short reset - Long clock mode",
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
    ],
    text: "This is a multimode envelope generator offering AD, ASR, and looping AD modes. Using the buttons, Attack and Decay curves are individually adjustable. Shift + Button 2 switches between modes: AD (yellow), ASR (blue), and looping AD (pink). Shift + Button 1 provides a manual trigger, Shift + Fader 1 sets the trigger-to-gate timing, and Shift + Fader 2 controls attenuation. The envelope can also be triggered via MIDI, with the MIDI channel set in the parameters. An internal trigger-to-gate converter defines how long the gate stays active, ranging from 0 to 4 seconds—at maximum time, the gate remains on indefinitely. This timing behaves differently depending on the selected envelope mode: in AD mode, it prevents retriggering until the timer runs out; in ASR mode, it holds the envelope for the set duration; and in looping AD mode, it loops the envelope for the timer duration, with infinite looping at maximum time, effectively turning it into an LFO. MIDI note triggering is supported on a user-defined channel, allowing you to save channels by using MIDI directly instead of relying on a MIDI-to-CV gate. The 'MIDI retrigger' parameter allow for the envelope to be retriggered when MIDI notes are overlapping",
    channels: [
      {
        jackTitle: "Gate Input",
        jackDescription: "Gate is detected if the voltage is above 1V",
        faderTitle: "Attack time",
        faderDescription: "Sets the attack time from 0 to 4 sec",
        faderPlusShiftTitle: "Trigger to gate time",
        faderPlusShiftDescription: "0-4 sec. Infinite at maximum.",
        fnTitle: "Attack curve selection",
        fnDescription:
          "Linear (yellow), logarithmic (pink), exponential (blue)",
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
        fnTitle: "Decay curve selection",
        fnDescription:
          "Linear (yellow), logarithmic (pink), exponential (blue)",
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
    params: ["Bipolar", "MIDI Channel", "MIDI CC"],
    storage: ["Speed", "Muted", "Attenuation", "Slew", "Clocked"],
    text: "This app sends random CC and CV values at regular intervals, either in free-running mode or synced to a clock. The timing is set using the fader, and the MIDI channel and CC number can be configured in the parameters. Shift + Fader attenuates both CV and CC outputs, while Button + Fader accesses the onboard slew limiter, which smooths changes in both CV and CC values. Shift + Button toggles mute/unmute for the outputs. The output range can be set to unipolar or bipolar in the parameters, which also determines the mute behavior—settling at 0 in unipolar mode and in the middle in bipolar mode, similar to the Control app. Shift + Button long press switches between free-running and tempo-synced operation.",
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
        fnTitle: "",
        fnDescription: "",
        fnPlusShiftTitle: "Mute - Clocked mode",
        fnPlusShiftDescription: "Short mute - Long clock mode",
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
    ],
    storage: [
      "Sequences (Gate/CV)",
      "Legato",
      "Sequence lengths",
      "Sequence resolution",
      "Gate lengths",
      "Ranges",
      "Octaves",
    ],
    text: "4x16 step sequencer app featuring four independent sequencers, each represented by a distinct color. Each sequencer has two pages, and you can navigate between them using Shift + Buttons. The CV/Gate outputs are paired per sequencer: jacks 1&2 for sequencer 1, 3&4 for sequencer 2, and so on. MIDI channels for each sequencer can be set individually in the parameters. Faders are used to set note values, buttons define the gate pattern, and long button presses enable legato. Shift modifies settings for the selected sequencer: Shift + Fader 1 sets step length, Fader 2 sets gate length, Fader 3 selects octave, Fader 4 defines the sequence range (1–5 octaves), and Fader 5 sets the sequence resolution (32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th). Buttons are used to select pages, with two pages available per sequencer. The output of each sequencer is quantized to the scale set in the global quantizer.",
    channels: [
      {
        jackTitle: "CV Output",
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Sequence length",
        faderPlusShiftDescription:
          "Set the length of the selected sequencer between 1 and 16 steps",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 1, page 1",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Quantized output",
        faderTitle: "Note",
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
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Octave",
        faderPlusShiftDescription: "offset the whole sequence by 0-5 Octaves",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 2, page 1",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Sequence Range",
        faderPlusShiftDescription: "set sequence range (1-5 octave)",
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
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
        faderPlusShiftTitle: "Sequence speed",
        faderPlusShiftDescription:
          "Set sequence resolution  32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 3, page 1",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",

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
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
        fnTitle: "Gate/Legato",
        fnDescription:
          "Short press sets a gate or rest, long press sets a legato",
        fnPlusShiftTitle: "Select Seq 4, page 1",
        ledTop: "Note level",
        ledTopPlusShift: "Sequence Length",
        ledBottom: "Active page",
        ledBottomPlusShift: "Sequence Length",
      },
      {
        jackTitle: "Gate Output",
        jackDescription: "Quantized output",
        faderTitle: "Note",
        faderDescription: "Sets the note at this step",
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
    ],
    storage: ["Attenuation", "Length", "Register", "Resolution"],
    text: "This app is inspired by the concept of a Turing machine as used in modular synthesizers—a type of probabilistic sequencer that generates evolving patterns based on controlled randomness. It can be set to send either MIDI CC or MIDI notes, while CV output is always active, sending 0–10V. The fader controls the probability of bit flips: when fully down, the sequence loops without changes; when fully up, bit flips occur constantly and the sequence length doubles; in the middle, there’s a 50/50 chance of flipping, resulting in the most randomness. Holding Shift and pressing the button a number of times sets the sequence length—for example, holding Shift and pressing three times sets a 3-step sequence, which is applied upon releasing Shift. The output is quantized for both CV and MIDI notes according to the global quantizer. Parameters include MIDI channel, base note (lowest MIDI note the Turing machine can generate), gate percentage (MIDI only), and color. Main functions include using the fader to set probability, Shift + Fader to set range, Shift + Button to set sequence length, and Button + Fader to set clock resolution (32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th). All app state is stored in scenes, including the sequences themselves—making this, as far as we know, the only Turing machine with preset saving.",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "0 to 10V CV",
        faderTitle: "Probability",
        faderDescription:
          "Bottom: no bit flip, Top: constant bit flips and doubled sequence length; Middle: max randomness",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Reduces the output range",
        faderPlusFnTitle: "Speed",
        faderPlusFnDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th",
        fnPlusShiftTitle: "Sequence Length",
        fnPlusShiftDescription: "Press button x times sets length to x",
        ledTop: "Output level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "",
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
    params: ["MIDI mode", "Midi channel", "CC number", "Color", "Range"],
    storage: ["Attenuation", "Length", "Register"],
    text: "Similar to the previous one, this is a classic Turing machine but extended to use two slots. The first jack is a clock input and the second is the CV output. The physical clock input allows for non-linear timing, custom dividers, or interaction with MIDI note lengths. The app can send either MIDI CC or MIDI notes, while CV output is always active, sending 0–10V. MIDI note on messages are sent on rising edges and note off messages on falling edges. Main functions: Fader 1 sets probability, Fader 2 sets output range. Shift + Button sets sequence length. The output is quantized by the global quantizer.",
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
    text: "This app is a Euclidean sequencer with two outputs: Jack 1 delivers the main Euclidean rhythm, while Jack 2 provides either an inverted version or an end-of-rhythm pulse. In inverted mode, if Output 1 sends a pulse, Output 2 does not—and vice versa. Send MIDI triggers, with MIDI channel and MIDI notes. Main functions include Fader 1 for sequence length and Fader 2 for number of beats (fill). Button 1 toggles semitone offset, Button 2 mutes the output. Shift + Fader 1 sets rotation, Shift + Fader 2 sets probability. Button + Fader 1 changes the sequencer speed with available resolutions: 32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar.",
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
        ledBottomPlusFn: "Clock speed",
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
    text: "This app sends random trigger signals on clock. It can output MIDI notes and CV triggers. The fader sets the probability of a trigger occurring at each clock pulse. The button acts as a mute toggle. Shift + Fader sets the clock resolution, allowing for rhythmic variation.",
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
    text: "This app sends MIDI notes and V/Oct voltages in a 0–10V range. The outputted notes are filtered by the global quantizer. The note value is tied to the fader position, with the range set by the span parameter. In clocked mode, the button is a toggle and the app outputs notes on regular intervals set by Button + Fader. In direct mode, the MIDI notes are sent when the button is pressed. Main functions: Fader sets the note; Shift + Fader sets clock resolution; Shift + Button toggles mode—Bottom LED is flashing for clocked mode, off for direct mode.",
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
        ledBottom: "Flashes in clocked mode",
      },
    ],
  },
  {
    appId: 11,
    title: "Offset + Attenuverter",
    description: "Offset and attenuverter module",
    color: "Rose",
    icon: "attenuate",
    params: ["Color"],
    storage: ["Attenuation", "Offset", "Offset toggle", "Attenuation toggle"],
    text: "This app provides offset and attenuverter functionality. The input and output range is ±5V, and the attenuverter has a maximum gain of 2x. Color can be set in the configurator. Jack 1 is the input, Jack 2 is the output. Main functions include Fader 1 for offset and Fader 2 for attenuvertion. Button 1 toggles the offset on or off, Button 2 toggles the attenuvertion on or off. When both of these are off the app acts as a simple pass through.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V signals",
        faderTitle: "Offset",
        faderDescription: "Applies a DC offset to the input signal",
        fnTitle: "Kill Offset",
        fnDescription: "Button 1 disables the offset",
        ledTop: "Positive input",
        ledBottom: "Negative input",
      },
      {
        jackTitle: "Output",
        jackDescription: "Outputs ±5V signals",
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
    params: ["Color"],
    storage: ["Attack", "Attenuvertion", "Offset"],
    text: "This app combines a slew limiter with offset and attenuverter functions. Input and output range is ±5V. Jack 1 is the input, Jack 2 is the output. Color can be set in the configurator. Main functions include Fader 1 for attack and Fader 2 for decay. Shift + Fader 1 sets offset, Shift + Fader 2 sets attenuvertion. Button 1 kills the offset, Button 2 sets the attenuvertion.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V signals",
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
        jackDescription: "Outputs ±5V signals",
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
    storage: ["Attack", "Attenuvertion", "Offset", "Input Gain"],
    text: "This app is an envelope follower with input and output ranges of ±5V. Jack 1 is the input, Jack 2 is the output. It includes offset and attenuverter functionality, making it ideal for driving VCAs or implementing sidechain compression. The attenuverter has a maximum gain of 2x. Main functions include Fader 1 for attack and Fader 2 for decay. Shift + Fader 1 sets offset, Shift + Fader 2 sets attenuvertion. Button 1 kills the offset, Button 2 sets the attenuvertion. Button 1 + Fader 1 adjusts input gain from 1x to 3x.",
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
        fnTitle: "Kill Offset",
        fnDescription: "Button 1 disables the offset",
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
        fnTitle: "Set Attenuverter",
        fnDescription: "Button 2 enables or configures the attenuvertion",
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
    params: ["Color"],
    storage: ["Octave shift", "Semitone shift", "Offset toggles"],
    text: "This app is a simple quantizer that processes CV signals within a ±5V range. Jack 1 is the input, Jack 2 is the output. The quantizer applies pitch quantization to the incoming CV. Fader 1 performs semitone shifts (0–12 semitones), and Fader 2 performs octave shifts (±5 octaves). These shifts are applied before quantization. Button 1 toggles semitone shift, and Button 2 toggles octave shift.",
    channels: [
      {
        jackTitle: "Input",
        jackDescription: "Accepts ±5V CV signals",
        faderTitle: "Semitone Shift",
        faderDescription: "Shifts the CV by 0–12 semitones before quantization",
        fnTitle: "Toggle Semitone Shift",
        fnDescription: "Enables/disables semitone shift",
        ledTop: "Displays semitone level",
        ledBottom: "",
      },
      {
        jackTitle: "Output",
        jackDescription: "Outputs quantized ±5V CV signals",
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
    params: ["Bipolar", "MIDI Channel", "MIDI CC", "Color"],
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
    params: ["Bipolar", "MIDI Channel", "Delay (ms)", "Color"],
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
    params: ["MIDI Channel", "MIDI Note", "GATE %", "Color"],
    storage: ["Division", "Muted", "Maximum division", "Minimum division"],
    text: "This is a simple clock divider app that was suggested by youtuber and Discord member Synthdad. The app allows for a performative control of clock division/multiplication allowing for 'build ups and drops' for example. The maximum and minimum divisions can be user set using shift + fader and button + fader respectively. These are saved into the scenes allowing you to set different ranges depending on your needs.",
    channels: [
      {
        jackTitle: "Trigger out",
        jackDescription: "Sends triggers on clock",
        faderTitle: "Division",
        faderDescription:
          "32ndT, 32nd, 16thT, 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, bar, 2 bars, 4 bars",
        faderPlusFnTitle: "Minimum division",
        faderPlusShiftTitle: "Maximum division",
        fnTitle: "",
        fnDescription: "",
        fnPlusShiftTitle: "Mute",
        ledTop: "Trigger activity indicator",
        ledTopPlusShift: "Maximum division",
        ledBottomPlusShift: "Minimum division",
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
      "MIDI CC 1",
      "MIDI CC 2",
      "Mute on release",
      "Color",
      "Store state",
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
    appId: 22,
    title: "LFO+",
    description: "Multi shape LFO",
    color: "Yellow",
    icon: "sine",
    params: ["Speed division", "Range", "Send MIDI", "MIDI Channel", "MIDI CC"],
    storage: [
      "CV attenuation",
      "CV mute",
      "CV destination",
      "Clocked",
      "Attenuation",
      "Speed",
      "Waveform",
    ],
    text: "This app is a variation of the simple LFO, adding an assignable CV input. The first channel processes the CV input, with the fader controlling its attenuation and the button acting as a mute. Use Shift + Button 1 to set the CV destination, indicated by the button color: speed (yellow), phase (pink), amplitude (cyan) or reset (red). Note that the speed CV is through 0, meaning that the waveform will invert and speed up again when the CV input is negative. When in 'reset' mode, the LFO resets when a rising edge passing the 1V threshold is detected. It is also worth knowing that even in reset mode this input is affected by the CV attenuation and 'mute' state allowing to change the rising edge detection level. As in the standard LFO, you can select the waveform by pressing the second button, with LED colors showing the shape: sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), and square (white). Adjust the CV output range using Shift + Fader. Shift + short press resets the waveform, while Shift + long press toggles between free-running and tempo-synced modes. Free-running speed ranges from 14 Hz to one cycle per minute; clocked mode offers resolutions like 16th, 8thT, 8th, 4thT, 4th, 2nd, note, half bar, and bar. App parameters let you set overall speed—Normal, Slow (÷2), or Slowest (÷4)—which also applies to clocked speeds. When clocked, the button flashes in sync with the LFO rate. Output can be bipolar (-5V to +5V) or unipolar (0V to 10V), affecting the attenuator's center when sending MIDI CC (0 for unipolar, 64 for bipolar). The app can also output MIDI CC, with freely configurable channel and CC number.",
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
        fnTitle: "Waveform selection",
        fnDescription:
          "Sine (yellow), triangle (pink), ramp down (cyan), ramp up (red), and square (white)",
        fnPlusShiftTitle: "Reset - Clocked mode",
        fnPlusShiftDescription: "Short reset - Long clock mode",
        ledTop: "Positive level indicator",
        ledTopPlusShift: "Attenuation level in red",
        ledBottom: "Negative level indicator",
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
          </li>
          <li>
            <Link to="#configurator">Configurator</Link>
          </li>
          <li>
            <Link to="#apps">Apps</Link>
            <List>
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
