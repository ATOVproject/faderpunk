import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface NoteParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly string[];
}

const noteLabels: Record<string, string> = {
  C: "C",
  CSharp: "C#",
  D: "D",
  DSharp: "D#",
  E: "E",
  F: "F",
  FSharp: "F#",
  G: "G",
  GSharp: "G#",
  A: "A",
  ASharp: "A#",
  B: "B",
};

export function NoteParamInput({ variants, ...props }: NoteParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant) => (
        <SelectItem key={variant}>{noteLabels[variant] || variant}</SelectItem>
      ))}
    </Select>
  );
}
