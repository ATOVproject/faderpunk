import type { MidiOutMode } from "@atov/fp-config";
import { Checkbox } from "@heroui/checkbox";
import { Select, SelectItem } from "@heroui/select";
import { Switch } from "@heroui/switch";
import { Controller, useFormContext } from "react-hook-form";

import { selectProps } from "../input/defaultProps";
import type { Inputs } from "../SettingsTab";

interface MidiOutModeItem {
  key: MidiOutMode["tag"];
  value: string;
  description: string;
}

const midiOutModeItems: MidiOutModeItem[] = [
  { key: "None", value: "None", description: "Disabled" },
  { key: "Local", value: "Local", description: "Local messages only" },
  {
    key: "MidiThru",
    value: "MIDI Thru",
    description: "Pass through MIDI from selected sources",
  },
  {
    key: "MidiMerge",
    value: "MIDI Merge",
    description: "Merge MIDI from selected sources with local messages",
  },
];

const MIDI_OUTPUTS = [
  { key: "usb", label: "USB", index: 0 },
  { key: "out1", label: "Out 1", index: 1 },
  { key: "out2", label: "Out 2", index: 2 },
] as const;

export const MidiSettings = () => {
  const { control, register, watch } = useFormContext<Inputs>();

  const midiUsbMode = watch("midiUsbMode");
  const midiOut1Mode = watch("midiOut1Mode");
  const midiOut2Mode = watch("midiOut2Mode");

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">MIDI</h2>
      <div className="flex flex-col gap-6">
        {MIDI_OUTPUTS.map((output) => {
          const prefix =
            output.key === "usb"
              ? "Usb"
              : output.key === "out1"
                ? "Out1"
                : "Out2";
          const modeKey = `midi${prefix}Mode` as keyof Inputs;
          const mode =
            output.key === "usb"
              ? midiUsbMode
              : output.key === "out1"
                ? midiOut1Mode
                : midiOut2Mode;

          return (
            <div key={output.key}>
              <h3 className="text-yellow-fp mb-2 text-xs font-semibold uppercase">
                {output.label}
              </h3>
              <div className="grid grid-cols-4 items-start gap-x-16 px-4">
                <Controller
                  name={modeKey}
                  control={control}
                  render={({ field }) => (
                    <Select
                      selectedKeys={[String(field.value)]}
                      onSelectionChange={(value) => {
                        field.onChange(value.currentKey);
                      }}
                      {...selectProps}
                      label="Mode"
                      items={midiOutModeItems}
                      placeholder="Mode"
                    >
                      {(item: MidiOutModeItem) => (
                        <SelectItem key={item.key} textValue={item.value}>
                          <div className="font-medium">{item.value}</div>
                          <div className="text-default-400 text-xs">
                            {item.description}
                          </div>
                        </SelectItem>
                      )}
                    </Select>
                  )}
                />

                <Switch
                  {...register(`midi${prefix}SendClock` as keyof Inputs)}
                  color="secondary"
                  classNames={{
                    base: "flex-col-reverse items-start justify-start",
                    label: "ms-0 mb-2 text-sm font-medium",
                  }}
                >
                  Send Clock
                </Switch>

                <Switch
                  {...register(`midi${prefix}SendTransport` as keyof Inputs)}
                  color="secondary"
                  classNames={{
                    base: "flex-col-reverse items-start justify-start",
                    label: "ms-0 mb-2 text-sm font-medium",
                  }}
                >
                  Send Transport
                </Switch>

                {(mode === "MidiThru" || mode === "MidiMerge") && (
                  <div className="flex flex-col">
                    {output.key === "usb" ? (
                      <>
                        <p className="mb-2 text-sm font-medium">Sources</p>
                        <p className="text-default-500 text-xs">
                          DIN routed to USB
                        </p>
                      </>
                    ) : (
                      <>
                        <p className="mb-2 text-sm font-medium">Sources</p>
                        <div className="flex flex-row gap-4">
                          <Checkbox
                            {...register(
                              `midi${prefix}SourceUsb` as keyof Inputs,
                            )}
                            color="secondary"
                          >
                            USB
                          </Checkbox>
                          <Checkbox
                            {...register(
                              `midi${prefix}SourceDin` as keyof Inputs,
                            )}
                            color="secondary"
                          >
                            DIN
                          </Checkbox>
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
