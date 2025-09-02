import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface ColorParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly (
    | "White"
    | "Yellow"
    | "Orange"
    | "Red"
    | "Lime"
    | "Green"
    | "Cyan"
    | "SkyBlue"
    | "Blue"
    | "Violet"
    | "Pink"
    | "PaleGreen"
    | "Sand"
    | "Rose"
    | "Salmon"
    | "LightBlue"
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

