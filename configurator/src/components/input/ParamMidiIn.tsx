import { useCallback } from "react";
import { type FieldValues, Controller, type Control } from "react-hook-form";
import { CheckboxGroup, Checkbox } from "@heroui/checkbox";
import { type FixedLengthArray } from "@atov/fp-config";

import { MIDI_IN_OPTIONS } from "../../utils/midiTypes";

interface Props {
  defaultValue: FixedLengthArray<boolean, 2>;
  name: string;
  paramIndex: number;
  control: Control<FieldValues>;
}

export const ParamMidiIn = ({
  defaultValue,
  name,
  paramIndex,
  control,
}: Props) => {
  const getSelectedKeys = (value: FixedLengthArray<boolean, 2>) => {
    const selected: string[] = [];
    MIDI_IN_OPTIONS.forEach((opt) => {
      if (value[opt.index]) {
        selected.push(opt.key);
      }
    });
    return selected;
  };

  const updateValue = useCallback((selected: string[]) => {
    const newValue: [boolean, boolean] = [false, false];
    MIDI_IN_OPTIONS.forEach((opt) => {
      if (selected.includes(opt.key)) {
        newValue[opt.index] = true;
      }
    });
    return newValue;
  }, []);

  return (
    <Controller
      name={`param-MidiIn-${paramIndex}`}
      control={control}
      defaultValue={defaultValue}
      render={({ field: { onChange, value } }) => (
        <CheckboxGroup
          label={name}
          value={getSelectedKeys(value)}
          onValueChange={(selected: string[]) =>
            onChange(updateValue(selected))
          }
          classNames={{
            label: "text-sm font-semibold text-white",
          }}
          orientation="horizontal"
        >
          {MIDI_IN_OPTIONS.map((option) => (
            <Checkbox
              classNames={{
                label: "text-sm",
              }}
              key={option.key}
              value={option.key}
            >
              {option.label}
            </Checkbox>
          ))}
        </CheckboxGroup>
      )}
    />
  );
};
