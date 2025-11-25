import type { MidiIn, MidiOut, MidiMode } from "@atov/fp-config";

// Extract tag types from the union types
export type MidiInTag = MidiIn["tag"];
export type MidiOutTag = MidiOut["tag"];
export type MidiModeTag = MidiMode["tag"];

// Create arrays of all possible tags
export const MIDI_IN_VARIANTS: MidiInTag[] = ["None", "All", "Din", "Usb"];

export const MIDI_OUT_VARIANTS: MidiOutTag[] = [
  "None",
  "All",
  "Out1",
  "Out2",
  "Usb",
  "Out1Usb",
  "Out2Usb",
  "Out1Out2",
];

export const MIDI_MODE_VARIANTS: MidiModeTag[] = ["Note", "Cc"];
