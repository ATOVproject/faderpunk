import { useCallback } from "react";
import { type FieldValues, Controller, type Control } from "react-hook-form";
import { CheckboxGroup, Checkbox } from "@heroui/checkbox";
import { type FixedLengthArray } from "@atov/fp-config";

import { MIDI_OUT_OPTIONS } from "../../utils/midiTypes";
import { useStore } from "../../store";

interface Props {
  defaultValue: FixedLengthArray<boolean, 3>;
  name: string;
  paramIndex: number;
  control: Control<FieldValues>;
}

export const ParamMidiOut = ({
  defaultValue,
  name,
  paramIndex,
  control,
}: Props) => {
  const config = useStore((state) => state.config);

  const isOutputDisabled = useCallback(
    (index: number): boolean => {
      if (!config) return false;
      const mode = config.midi.outs[index].mode;
      return mode.tag === "None" || mode.tag === "MidiThru";
    },
    [config],
  );

  const getSelectedKeys = useCallback(
    (value: FixedLengthArray<boolean, 3>) => {
      const selected: string[] = [];
      MIDI_OUT_OPTIONS.forEach((opt) => {
        // Don't show disabled outputs as selected
        if (value[opt.index] && !isOutputDisabled(opt.index)) {
          selected.push(opt.key);
        }
      });
      return selected;
    },
    [isOutputDisabled],
  );

  const updateValue = useCallback((selected: string[]) => {
    const newValue: [boolean, boolean, boolean] = [false, false, false];
    MIDI_OUT_OPTIONS.forEach((opt) => {
      if (selected.includes(opt.key)) {
        newValue[opt.index] = true;
      }
    });
    return newValue;
  }, []);

  return (
    <Controller
      name={`param-MidiOut-${paramIndex}`}
      control={control}
      defaultValue={defaultValue}
      render={({ field: { onChange, value } }) => (
        <CheckboxGroup
          className="max-w-50"
          disableAnimation
          classNames={{
            label: "text-sm font-semibold text-white",
          }}
          radius="sm"
          label={name}
          value={getSelectedKeys(value)}
          onValueChange={(selected: string[]) =>
            onChange(updateValue(selected))
          }
          orientation="horizontal"
        >
          {MIDI_OUT_OPTIONS.map((option) => (
            <Checkbox
              key={option.key}
              value={option.key}
              isDisabled={isOutputDisabled(option.index)}
              classNames={{
                label: "text-sm",
              }}
            >
              {option.label}
            </Checkbox>
          ))}
        </CheckboxGroup>
      )}
    />
  );
};
