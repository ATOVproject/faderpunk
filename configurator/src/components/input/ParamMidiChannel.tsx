import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Input } from "@heroui/input";

import { inputProps } from "./defaultProps";

interface Props {
  paramIndex: number;
  name: string;
  defaultValue: string;
  register: UseFormRegister<FieldValues>;
}

export const ParamMidiChannel = ({
  defaultValue,
  name,
  paramIndex,
  register,
}: Props) => (
  <Input
    defaultValue={defaultValue}
    {...register(`param-MidiChannel-${paramIndex}`)}
    {...inputProps}
    min={1}
    max={16}
    type="number"
    label={name}
  />
);
