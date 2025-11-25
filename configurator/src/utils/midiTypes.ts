import type { MidiMode } from "@atov/fp-config";

// MidiIn and MidiOut are now boolean arrays, not tag-based enums
// MidiIn: [usb, din]
// MidiOut: [usb, out1, out2]

export const MIDI_IN_OPTIONS = [
  { key: "usb", label: "USB", index: 0 },
  { key: "din", label: "DIN", index: 1 },
] as const;

export const MIDI_OUT_OPTIONS = [
  { key: "usb", label: "USB", index: 0 },
  { key: "out1", label: "Out 1", index: 1 },
  { key: "out2", label: "Out 2", index: 2 },
] as const;

// MidiMode is still tag-based
export type MidiModeTag = MidiMode["tag"];
export const MIDI_MODE_VARIANTS: MidiModeTag[] = ["Note", "Cc"];
