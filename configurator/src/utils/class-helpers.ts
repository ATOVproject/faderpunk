import { type Color } from "@atov/fp-config";

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

type AllColors = Color["tag"] | "Black";

export const COLORS_CLASSES: Record<AllColors, string> = {
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
  Black: "bg-black",
};
