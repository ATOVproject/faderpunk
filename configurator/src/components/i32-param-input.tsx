import { Input } from "@heroui/input";
import { ComponentProps } from "react";

interface I32ParamInputProps
  extends Omit<ComponentProps<typeof Input>, "onChange"> {
  min: number;
  max: number;
  onChange?: (value: number) => void;
}

export function I32ParamInput({
  min,
  max,
  onChange,
  ...props
}: I32ParamInputProps) {
  return (
    <Input
      {...props}
      max={max}
      min={min}
      type="number"
      onChange={(e) => {
        const numValue = parseInt(e.target.value, 10);

        if (!isNaN(numValue) && onChange) {
          onChange(numValue);
        }
      }}
    />
  );
}
