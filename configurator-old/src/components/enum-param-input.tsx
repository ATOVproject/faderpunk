import { Select, SelectItem } from "@heroui/select";
import { ComponentProps } from "react";

interface EnumParamInputProps
  extends Omit<ComponentProps<typeof Select>, "children"> {
  variants: readonly string[];
}

export function EnumParamInput({ variants, ...props }: EnumParamInputProps) {
  return (
    <Select {...props}>
      {variants.map((variant, index) => (
        <SelectItem key={index}>{variant}</SelectItem>
      ))}
    </Select>
  );
}
