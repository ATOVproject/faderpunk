import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface CurveParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly ("Linear" | "Exponential" | "Logarithmic")[];
}

export function CurveParamInput({ variants, ...props }: CurveParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant) => (
        <SelectItem key={variant}>{variant}</SelectItem>
      ))}
    </Select>
  );
}
