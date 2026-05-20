// Static app catalog for simulator mode.
// TODO: Auto-regenerate this file on every release (GitHub Action running gen-bindings).
import type { Param } from "@atov/fp-config";
import type { AllApps, App } from "../utils/types";

const COLOR_8: Param = {
  tag: "Color",
  value: {
    name: "Color",
    variants: [
      { tag: "Blue" },
      { tag: "Green" },
      { tag: "Rose" },
      { tag: "Orange" },
      { tag: "Cyan" },
      { tag: "Pink" },
      { tag: "Violet" },
      { tag: "Yellow" },
    ],
  },
};

const CURVE_3: Param = {
  tag: "Curve",
  value: {
    name: "Curve",
    variants: [
      { tag: "Linear" },
      { tag: "Logarithmic" },
      { tag: "Exponential" },
    ],
  },
};

const RANGE_2V: Param = {
  tag: "Range",
  value: {
    name: "Range",
    variants: [{ tag: "_0_10V" }, { tag: "_Neg5_5V" }],
  },
};

const RANGE_3V: Param = {
  tag: "Range",
  value: {
    name: "Range",
    variants: [{ tag: "_0_10V" }, { tag: "_0_5V" }, { tag: "_Neg5_5V" }],
  },
};

const midiChannel = (name = "MIDI Channel"): Param => ({
  tag: "MidiChannel",
  value: { name },
});

const midiCc = (name = "MIDI CC"): Param => ({
  tag: "MidiCc",
  value: { name },
});

const midiNote = (name = "MIDI Note"): Param => ({
  tag: "MidiNote",
  value: { name },
});

const bool = (name: string): Param => ({ tag: "bool", value: { name } });

const enumParam = (name: string, variants: string[]): Param => ({
  tag: "Enum",
  value: { name, variants },
});

const i32 = (name: string, min: number, max: number): Param => ({
  tag: "i32",
  value: { name, min, max },
});

const GATE_PCT = i32("GATE %", 1, 100);
const MIDI_IN: Param = { tag: "MidiIn" };
const MIDI_OUT: Param = { tag: "MidiOut" };
const MIDI_NRPN: Param = { tag: "MidiNrpn" };
const MIDI_MODE: Param = { tag: "MidiMode" };

const makeApp = (
  appId: number,
  channels: number,
  color: App["color"],
  icon: App["icon"],
  name: string,
  description: string,
  params: Param[],
): App => ({
  appId,
  channels: BigInt(channels),
  color,
  icon,
  name,
  description,
  paramCount: BigInt(params.length),
  params,
});

const APPS: App[] = [
  makeApp(1, 1, "Violet", "Fader", "Control", "Simple MIDI/CV controller", [
    CURVE_3,
    RANGE_2V,
    midiChannel(),
    midiCc(),
    bool("Mute on release"),
    bool("Invert"),
    COLOR_8,
    bool("Store state"),
    enumParam("Button mode", ["Mute", "CC toggle", "CC momentary"]),
    midiChannel("Button Channel"),
    midiCc("Button CC"),
    MIDI_NRPN,
    MIDI_OUT,
  ]),
  makeApp(2, 1, "Yellow", "Sine", "LFO", "Multi shape LFO", [
    enumParam("Speed", ["Normal", "Slow", "Slowest"]),
    RANGE_2V,
    midiChannel(),
    midiCc(),
    MIDI_NRPN,
    MIDI_OUT,
  ]),
  makeApp(
    3,
    2,
    "Yellow",
    "AdEnv",
    "AD Envelope",
    "Variable curve AD, ASR or looping AD",
    [MIDI_IN, midiChannel(), bool("MIDI retrigger")],
  ),
  makeApp(
    4,
    1,
    "Green",
    "Random",
    "Random CC/CV",
    "Generate random CC and CV values",
    [RANGE_2V, midiChannel(), midiCc(), MIDI_NRPN, MIDI_OUT],
  ),
  makeApp(
    5,
    8,
    "Yellow",
    "Sequence",
    "Sequencer",
    "4 x 16 step CV/gate sequencers",
    [
      midiChannel("MIDI Channel 1"),
      midiChannel("MIDI Channel 2"),
      midiChannel("MIDI Channel 3"),
      midiChannel("MIDI Channel 4"),
      MIDI_OUT,
    ],
  ),
  makeApp(
    6,
    1,
    "Blue",
    "SequenceSquare",
    "Turing",
    "Turing machine, synched to internal clock",
    [
      MIDI_MODE,
      midiChannel("MIDI channel"),
      midiCc("CC number"),
      midiNote("Base Note"),
      GATE_PCT,
      COLOR_8,
      RANGE_3V,
      MIDI_NRPN,
      MIDI_OUT,
    ],
  ),
  makeApp(
    7,
    2,
    "Pink",
    "SequenceSquare",
    "Turing+",
    "Turing machine, with clock input",
    [
      MIDI_MODE,
      midiChannel(),
      midiCc("CC number"),
      COLOR_8,
      RANGE_3V,
      midiNote("Base note"),
      MIDI_NRPN,
      MIDI_OUT,
    ],
  ),
  makeApp(8, 2, "Orange", "Euclid", "Euclid", "Euclidean sequencer", [
    midiChannel(),
    midiNote("MIDI Note 1"),
    midiNote("MIDI NOTE 2"),
    GATE_PCT,
    COLOR_8,
    MIDI_OUT,
  ]),
  makeApp(
    9,
    1,
    "Cyan",
    "Die",
    "Random Triggers",
    "Generate random triggers on clock",
    [midiChannel(), midiNote(), GATE_PCT, COLOR_8, MIDI_OUT],
  ),
  makeApp(
    10,
    1,
    "Rose",
    "Note",
    "Note Fader",
    "Play MIDI notes manually or on clock",
    [
      midiChannel(),
      midiNote("Base note"),
      i32("Span", 1, 120),
      GATE_PCT,
      enumParam("Out", ["CV", "Gate"]),
      COLOR_8,
      MIDI_OUT,
    ],
  ),
  makeApp(
    11,
    2,
    "Rose",
    "Attenuate",
    "Offset+Attenuverter",
    "Offset and attenuvert CV",
    [COLOR_8],
  ),
  makeApp(12, 2, "Green", "SoftRandom", "Slew Limiter", "Slows CV changes", [
    COLOR_8,
  ]),
  makeApp(
    13,
    2,
    "Pink",
    "EnvFollower",
    "Envelope Follower",
    "Audio amplitude to CV",
    [COLOR_8, RANGE_2V],
  ),
  makeApp(
    14,
    2,
    "Blue",
    "Quantize",
    "Quantizer",
    "Quantize CV passing through",
    [COLOR_8],
  ),
  makeApp(
    15,
    1,
    "Cyan",
    "KnobRound",
    "MIDI to CV",
    "Multifunctional MIDI to CV",
    [
      enumParam("Mode", [
        "CC",
        "Pitch",
        "Gate",
        "Velocity",
        "AT",
        "Bend",
        "Note Gate",
      ]),
      CURVE_3,
      midiChannel(),
      midiCc(),
      i32("Bend Range", 1, 24),
      midiNote(),
      COLOR_8,
      MIDI_IN,
      bool("Velocity on Gate"),
    ],
  ),
  makeApp(16, 1, "Violet", "NoteGrid", "CV to MIDI", "CV to MIDI CC", [
    RANGE_2V,
    midiChannel(),
    midiCc(),
    COLOR_8,
    MIDI_NRPN,
    MIDI_OUT,
  ]),
  makeApp(
    17,
    2,
    "Orange",
    "NoteBox",
    "CV/OCT to MIDI",
    "CV and gate to MIDI note converter",
    [RANGE_2V, midiChannel(), i32("Delay (ms)", 0, 10), COLOR_8, MIDI_OUT],
  ),
  makeApp(
    18,
    1,
    "Orange",
    "NoteBox",
    "Clock Divider",
    "Simple clock divider",
    [
      midiChannel(),
      midiNote(),
      GATE_PCT,
      enumParam("Divisions", ["Straight", "Triplets", "Both"]),
      COLOR_8,
      MIDI_OUT,
    ],
  ),
  makeApp(
    19,
    2,
    "Blue",
    "Stereo",
    "Panner",
    "Use with 2 VCA to do panning or cross fading",
    [
      CURVE_3,
      RANGE_3V,
      midiChannel(),
      midiCc("MIDI CC 1"),
      midiCc("MIDI CC 2"),
      bool("Mute on release"),
      COLOR_8,
      bool("Store state"),
      MIDI_NRPN,
      MIDI_OUT,
    ],
  ),
  makeApp(
    20,
    2,
    "Green",
    "Random",
    "Random+",
    "Generate random CC and CV values with assignable CV input",
    [bool("Bipolar"), midiChannel(), midiCc(), MIDI_NRPN, MIDI_OUT, COLOR_8],
  ),
  makeApp(
    21,
    2,
    "Orange",
    "NoteBox",
    "Clock Divider+",
    "Clock divider with assignable CV input",
    [
      midiChannel(),
      midiNote(),
      GATE_PCT,
      enumParam("Divisions", ["Straight", "Triplets", "Both"]),
      COLOR_8,
      MIDI_OUT,
    ],
  ),
  makeApp(
    22,
    2,
    "Yellow",
    "Sine",
    "LFO+",
    "Multi shape LFO with CV input",
    [
      enumParam("Speed", ["Normal", "Slow", "Slowest"]),
      RANGE_2V,
      midiChannel(),
      midiCc(),
      COLOR_8,
      MIDI_NRPN,
      MIDI_OUT,
    ],
  ),
  makeApp(
    23,
    4,
    "SkyBlue",
    "Euclid",
    "FP Grids",
    "Topographic drum sequencer port of Mutable Instruments Grids, synced to internal clock",
    [
      midiNote("MIDI Note 1"),
      midiNote("MIDI Note 2"),
      midiNote("MIDI Note 3"),
      midiNote("MIDI DnB Ghost Note"),
      midiChannel("Note 1 MIDI Channel"),
      midiChannel("Note 2 MIDI Channel"),
      midiChannel("Note 3 MIDI Channel"),
      midiChannel("DnB Ghost Note MIDI Channel"),
      i32("MIDI Velocity", 1, 127),
      i32("MIDI Accent Vel", 1, 127),
      GATE_PCT,
      COLOR_8,
      MIDI_OUT,
    ],
  ),
];

export const DEMO_APPS: AllApps = new Map(APPS.map((a) => [a.appId, a]));
