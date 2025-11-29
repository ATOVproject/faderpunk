import { AllColors } from "./types";

export const WIDTHS_CLASSES: Record<number, string> = {
  1: "col-span-1",
  2: "col-span-2",
  3: "col-span-3",
  4: "col-span-4",
  5: "col-span-5",
  6: "col-span-6",
  7: "col-span-7",
  8: "col-span-8",
  9: "col-span-9",
  10: "col-span-10",
  11: "col-span-11",
  12: "col-span-12",
  13: "col-span-13",
  14: "col-span-14",
  15: "col-span-15",
  16: "col-span-16",
};

export const COLORS_CLASSES: Record<
  AllColors,
  { bg: string; text: string; border: string }
> = {
  Blue: { bg: "bg-blue", text: "text-blue", border: "border-blue" },
  Green: { bg: "bg-green", text: "text-green", border: "border-green" },
  Rose: { bg: "bg-red", text: "text-red", border: "border-red" },
  Orange: { bg: "bg-orange", text: "text-orange", border: "border-orange" },
  Cyan: { bg: "bg-cyan", text: "text-cyan", border: "border-cyan" },
  Violet: { bg: "bg-violet", text: "text-violet", border: "border-violet" },
  Pink: { bg: "bg-pink", text: "text-pink", border: "border-pink" },
  Yellow: { bg: "bg-yellow", text: "text-yellow", border: "border-yellow" },
  White: { bg: "bg-white", text: "text-white", border: "border-white" },
  Red: { bg: "bg-red", text: "text-red", border: "border-red" },
  Lime: { bg: "bg-green", text: "text-green", border: "border-green" },
  SkyBlue: { bg: "bg-blue", text: "text-blue", border: "border-blue" },
  PaleGreen: { bg: "bg-green", text: "text-green", border: "border-green" },
  Sand: { bg: "bg-yellow", text: "text-yellow", border: "border-yellow" },
  Salmon: { bg: "bg-pink", text: "text-pink", border: "border-pink" },
  LightBlue: { bg: "bg-blue", text: "text-blue", border: "border-blue" },
  Custom: {
    bg: "bg-transparent",
    text: "text-transparent",
    border: "border-transparent",
  },
  Black: { bg: "bg-black", text: "text-black", border: "border-black" },
};

// Quantizer color palette mappings
export const QUANTIZER_KEY_COLORS: Record<string, string> = {
  Chromatic: "bg-palette-white",
  Ionian: "bg-palette-pink",
  Dorian: "bg-palette-yellow",
  Phrygian: "bg-palette-cyan",
  Lydian: "bg-palette-salmon",
  Mixolydian: "bg-palette-lime",
  Aeolian: "bg-palette-orange",
  Locrian: "bg-palette-green",
  BluesMaj: "bg-palette-sky-blue",
  BluesMin: "bg-palette-red",
  PentatonicMaj: "bg-palette-pale-green",
  PentatonicMin: "bg-palette-blue",
  Folk: "bg-palette-sand",
  Japanese: "bg-palette-violet",
  Gamelan: "bg-palette-light-blue",
  HungarianMin: "bg-palette-rose",
};

export const QUANTIZER_TONIC_COLORS: Record<string, string> = {
  C: "bg-palette-white",
  CSharp: "bg-palette-pink",
  D: "bg-palette-yellow",
  DSharp: "bg-palette-cyan",
  E: "bg-palette-salmon",
  F: "bg-palette-lime",
  FSharp: "bg-palette-orange",
  G: "bg-palette-green",
  GSharp: "bg-palette-sky-blue",
  A: "bg-palette-red",
  ASharp: "bg-palette-pale-green",
  B: "bg-palette-blue",
};
