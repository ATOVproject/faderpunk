import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface RangeParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly string[];
}

const rangeLabels: Record<string, string> = {
  _0_10V: "0-10V",
  _0_5V: "0-5V",
  _Neg5_5V: "-5-5V",
};

export function RangeParamInput({ variants, ...props }: RangeParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant) => (
        <SelectItem key={variant}>{rangeLabels[variant]}</SelectItem>
      ))}
    </Select>
  );
}