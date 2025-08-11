import { Input } from "@heroui/input";
import { ComponentProps } from "react";

interface F32ParamInputProps extends Omit<ComponentProps<typeof Input>, 'onChange'> {
  onChange?: (value: number) => void;
}

export function F32ParamInput({ onChange, ...props }: F32ParamInputProps) {
  return (
    <Input
      {...props}
      type="number"
      onChange={(e) => {
        const numValue = parseFloat(e.target.value);
        if (!isNaN(numValue) && onChange) {
          onChange(numValue);
        }
      }}
    />
  );
}

