import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Input } from "@heroui/input";

import { inputProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  max: number;
  min: number;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

export const ParamF32 = ({
  defaultValue,
  max,
  min,
  name,
  paramIndex,
  register,
}: Props) => (
  <Input
    defaultValue={defaultValue}
    {...register(`param-f32-${paramIndex}`, { valueAsNumber: true })}
    {...inputProps}
    max={max}
    min={min}
    type="number"
    inputMode="decimal"
    step="any"
    label={name}
  />
);
