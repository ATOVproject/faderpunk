import type { MidiOutMode } from "@atov/fp-config";
import { SelectItem } from "@heroui/select";
import { useFormContext } from "react-hook-form";

import type { Inputs } from "../SettingsTab";
import {
  ControlledSelect,
  ControlledSwitch,
  ControlledCheckbox,
} from "./ControlledFields";

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

const switchClassNames = {
  base: "flex-col-reverse items-start justify-start",
  label: "ms-0 mb-2 text-sm font-medium",
};

export const MidiSettings = () => {
  const { control, watch } = useFormContext<Inputs>();

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
                <ControlledSelect
                  name={modeKey}
                  control={control}
                  items={midiOutModeItems}
                  label="Mode"
                  placeholder="Mode"
                >
                  {(item: { key: string; value: string }) => {
                    const modeItem = item as MidiOutModeItem;
                    return (
                      <SelectItem key={modeItem.key} textValue={modeItem.value}>
                        <div className="font-medium">{modeItem.value}</div>
                        <div className="text-default-400 text-xs">
                          {modeItem.description}
                        </div>
                      </SelectItem>
                    );
                  }}
                </ControlledSelect>

                <ControlledSwitch
                  name={`midi${prefix}SendClock` as keyof Inputs}
                  control={control}
                  switchProps={{
                    color: "secondary",
                    classNames: switchClassNames,
                  }}
                >
                  Send Clock
                </ControlledSwitch>

                <ControlledSwitch
                  name={`midi${prefix}SendTransport` as keyof Inputs}
                  control={control}
                  switchProps={{
                    color: "secondary",
                    classNames: switchClassNames,
                  }}
                >
                  Send Transport
                </ControlledSwitch>

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
                          <ControlledCheckbox
                            name={`midi${prefix}SourceUsb` as keyof Inputs}
                            control={control}
                            checkboxProps={{ color: "secondary" }}
                          >
                            USB
                          </ControlledCheckbox>
                          <ControlledCheckbox
                            name={`midi${prefix}SourceDin` as keyof Inputs}
                            control={control}
                            checkboxProps={{ color: "secondary" }}
                          >
                            DIN
                          </ControlledCheckbox>
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
