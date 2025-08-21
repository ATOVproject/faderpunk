import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface WaveformParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly ("Triangle" | "Saw" | "Rect" | "Sine")[];
}

export function WaveformParamInput({
  variants,
  ...props
}: WaveformParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant) => (
        <SelectItem key={variant}>{variant}</SelectItem>
      ))}
    </Select>
  );
}
