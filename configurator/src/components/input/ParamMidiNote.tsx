import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Input } from "@heroui/input";

import { inputProps } from "./defaultProps";

interface Props {
  paramIndex: number;
  name: string;
  defaultValue: string;
  register: UseFormRegister<FieldValues>;
}

export const ParamMidiNote = ({
  defaultValue,
  name,
  paramIndex,
  register,
}: Props) => (
  <Input
    defaultValue={defaultValue}
    {...register(`param-MidiNote-${paramIndex}`)}
    {...inputProps}
    min={0}
    max={127}
    type="number"
    label={name}
  />
);
