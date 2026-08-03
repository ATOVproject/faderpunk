// GENERATED FILE — do not edit by hand.
// Regenerate with `./gen-app-library.sh` from repo root. Source of truth:
// faderpunk/src/apps/mod.rs (order) and each app's `Config::new(...)` call.

export interface AppLibraryEntry {
  id: number;
  name: string;
  description: string;
  color: string;
  icon: string;
}

export const APP_LIBRARY: AppLibraryEntry[] = [
  {
    id: 1,
    name: "Control",
    description: "Simple MIDI/CV controller",
    color: "Violet",
    icon: "fader",
  },
  {
    id: 2,
    name: "LFO",
    description: "Multi shape LFO",
    color: "Yellow",
    icon: "sine",
  },
  {
    id: 3,
    name: "AD Envelope",
    description: "Variable curve AD, ASR or looping AD",
    color: "Yellow",
    icon: "ad-env",
  },
  {
    id: 4,
    name: "Random CC/CV",
    description: "Generate random CC and CV values",
    color: "Green",
    icon: "random",
  },
  {
    id: 5,
    name: "Sequencer",
    description: "4 x 16 step CV/gate sequencers",
    color: "Yellow",
    icon: "sequence",
  },
  {
    id: 6,
    name: "Turing",
    description: "Turing machine, synched to internal clock",
    color: "Blue",
    icon: "sequence-square",
  },
  {
    id: 7,
    name: "Turing+",
    description: "Turing machine, with clock input",
    color: "Pink",
    icon: "sequence-square",
  },
  {
    id: 8,
    name: "Euclid",
    description: "Euclidean sequencer",
    color: "Orange",
    icon: "euclid",
  },
  {
    id: 9,
    name: "Random Triggers",
    description: "Generate random triggers on clock",
    color: "Cyan",
    icon: "die",
  },
  {
    id: 10,
    name: "Note Fader",
    description: "Play MIDI notes manually or on clock",
    color: "Rose",
    icon: "note",
  },
  {
    id: 11,
    name: "Offset+Attenuverter",
    description: "Offset and attenuvert CV",
    color: "Rose",
    icon: "attenuate",
  },
  {
    id: 12,
    name: "Slew Limiter",
    description: "Slows CV changes",
    color: "Green",
    icon: "soft-random",
  },
  {
    id: 13,
    name: "Envelope Follower",
    description: "Audio amplitude to CV",
    color: "Pink",
    icon: "env-follower",
  },
  {
    id: 14,
    name: "Quantizer",
    description: "Quantize CV passing through",
    color: "Blue",
    icon: "quantize",
  },
  {
    id: 15,
    name: "MIDI to CV",
    description: "Multifunctional MIDI to CV",
    color: "Cyan",
    icon: "knob-round",
  },
  {
    id: 16,
    name: "CV to MIDI",
    description: "CV to MIDI CC",
    color: "Violet",
    icon: "note-grid",
  },
  {
    id: 17,
    name: "CV/OCT to MIDI",
    description: "CV and gate to MIDI note converter",
    color: "Orange",
    icon: "note-box",
  },
  {
    id: 18,
    name: "Clock Divider",
    description: "Simple clock divider",
    color: "Orange",
    icon: "note-box",
  },
  {
    id: 19,
    name: "Panner",
    description: "Use with 2 VCA to do panning or cross fading",
    color: "Blue",
    icon: "stereo",
  },
  {
    id: 20,
    name: "Random+",
    description: "Generate random CC and CV values with assignable CV input",
    color: "Green",
    icon: "random",
  },
  {
    id: 21,
    name: "Clock Divider+",
    description: "Clock divider with assignable CV input",
    color: "Orange",
    icon: "note-box",
  },
  {
    id: 22,
    name: "LFO+",
    description: "Multi shape LFO with CV input",
    color: "Yellow",
    icon: "sine",
  },
  {
    id: 23,
    name: "FP Grids",
    description:
      "Topographic drum sequencer port of Mutable Instruments Grids, synced to internal clock",
    color: "SkyBlue",
    icon: "euclid",
  },
  {
    id: 24,
    name: "TB-3PO",
    description: "TB-303 acid pattern generator",
    color: "Orange",
    icon: "soft-random",
  },
  {
    id: 25,
    name: "Automator",
    description: "CV gesture looper",
    color: "Cyan",
    icon: "fader",
  },
  {
    id: 26,
    name: "GenSeq",
    description: "Generative sequencer with Turing machine registers",
    color: "Yellow",
    icon: "sequence",
  },
  {
    id: 27,
    name: "Bernoulli Gate",
    description: "Two-output Bernoulli gate synced to internal clock",
    color: "Cyan",
    icon: "die",
  },
];
