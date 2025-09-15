import { type Color } from "@atov/fp-config";

export const WIDTHS_CLASSES: Record<number, string> = {
  1: "grow-1",
  2: "grow-2",
  3: "grow-3",
  4: "grow-4",
  5: "grow-5",
  6: "grow-6",
  7: "grow-7",
  8: "grow-8",
  9: "grow-9",
  10: "grow-10",
  11: "grow-11",
  12: "grow-12",
  13: "grow-13",
  14: "grow-14",
  15: "grow-15",
  16: "grow-16",
};

type AllColors = Color["tag"] & "None";

export const COLORS_CLASSES: Record<Color["tag"], string> = {
  Blue: "bg-blue",
  Green: "bg-green",
  Rose: "bg-red",
  Orange: "bg-orange",
  Cyan: "bg-cyan",
  Violet: "bg-violet",
  Pink: "bg-pink",
  Yellow: "bg-yellow",
  White: "bg-white",
  Red: "bg-red",
  Lime: "bg-green",
  SkyBlue: "bg-blue",
  PaleGreen: "bg-green",
  Sand: "bg-yellow",
  Salmon: "bg-pink",
  LightBlue: "bg-blue",
  Custom: "bg-transparent",
  // None: "bg-transparent",
};
