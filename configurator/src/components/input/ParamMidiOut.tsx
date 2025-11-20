import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "./defaultProps";
import { MIDI_OUT_VARIANTS } from "../../utils/midiTypes";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

type Item = { key: string; value: string };

export const ParamMidiOut = ({
  defaultValue,
  name,
  paramIndex,
  register,
}: Props) => {
  const items = useMemo(
    () =>
      MIDI_OUT_VARIANTS.map((variant) => ({ key: variant, value: variant })),
    [],
  );

  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-MidiOut-${paramIndex}`)}
      {...selectProps}
      label={name}
      items={items}
      placeholder={name}
    >
      {(item: Item) => <SelectItem>{item.value}</SelectItem>}
    </Select>
  );
};
