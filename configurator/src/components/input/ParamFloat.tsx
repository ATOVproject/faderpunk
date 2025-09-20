import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Input } from "@heroui/input";

import { inputProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

export const ParamFloat = ({
  defaultValue,
  name,
  paramIndex,
  register,
}: Props) => (
  <Input
    defaultValue={defaultValue}
    {...register(`param-f32-${paramIndex}`)}
    {...inputProps}
    type="number"
    step="any"
    label={name}
  />
);
