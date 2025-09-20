import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Input } from "@heroui/input";

import { inputProps } from "./defaultProps";

interface Props {
  paramIndex: number;
  min: string | number;
  max: string | number;
  name: string;
  defaultValue: string;
  register: UseFormRegister<FieldValues>;
}

export const ParamI32 = ({
  defaultValue,
  max,
  min,
  name,
  paramIndex,
  register,
}: Props) => (
  <Input
    defaultValue={defaultValue}
    {...register(`param-i32-${paramIndex}`)}
    {...inputProps}
    min={min}
    max={max}
    type="number"
    label={name}
  />
);
