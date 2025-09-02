import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface ColorParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly (
    | "White"
    | "Red"
    | "Lime"
    | "RoyalBlue"
    | "Magenta"
    | "Cyan"
    | "Orange"
    | "Green"
    | "Violet"
    | "Pink"
    | "SpringGreen"
    | "Crimson"
    | "Yellow"
    | "Purple"
    | "Teal"
    | "Gold"
  )[];
}

export function ColorParamInput({ variants, ...props }: ColorParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant) => (
        <SelectItem key={variant}>{variant}</SelectItem>
      ))}
    </Select>
  );
}
