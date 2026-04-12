import type { ClockSrc, ResetSrc } from "@atov/fp-config";
import { SelectItem } from "@heroui/select";
import { Controller, useFormContext } from "react-hook-form";
import { Input } from "@heroui/input";
import classNames from "classnames";

import { inputProps } from "../input/defaultProps";
import { Icon } from "../Icon";
import type { Inputs } from "../SettingsTab";
import { ControlledSelect } from "./ControlledFields";

interface ClockSrcItem {
  key: ClockSrc["tag"];
  value: string;
  icon?: string;
  iconClass?: string;
}

interface ResetSrcItems {
  key: ResetSrc["tag"];
  value: string;
  icon?: string;
  iconClass?: string;
}

const clockSrcItems: ClockSrcItem[] = [
  { key: "None", value: "None" },
  { key: "Atom", value: "Atom", icon: "atom", iconClass: "text-cyan-fp" },
  {
    key: "Meteor",
    value: "Meteor",
    icon: "meteor",
    iconClass: "text-yellow-fp",
  },
  { key: "Cube", value: "Cube", icon: "cube", iconClass: "text-pink-fp" },
  { key: "Internal", value: "Internal", icon: "timer" },
  { key: "MidiIn", value: "MIDI In", icon: "midi" },
  { key: "MidiUsb", value: "MIDI USB", icon: "usb" },
];

const resetSrcItems: ResetSrcItems[] = [
  { key: "None", value: "None" },
  { key: "Atom", value: "Atom", icon: "atom", iconClass: "text-cyan-fp" },
  {
    key: "Meteor",
    value: "Meteor",
    icon: "meteor",
    iconClass: "text-yellow-fp",
  },
  { key: "Cube", value: "Cube", icon: "cube", iconClass: "text-pink-fp" },
];

export const ClockSettings = () => {
  const { control } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">Clock</h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <ControlledSelect
          name="clockSrc"
          control={control}
          items={clockSrcItems}
          label="Clock source"
          placeholder="Clock source"
        >
          {(item) => (
            <SelectItem
              startContent={
                (item as ClockSrcItem).icon ? (
                  <Icon
                    className={classNames(
                      "h-5 w-5",
                      (item as ClockSrcItem).iconClass,
                    )}
                    name={(item as ClockSrcItem).icon!}
                  />
                ) : undefined
              }
            >
              {item.value}
            </SelectItem>
          )}
        </ControlledSelect>
        <ControlledSelect
          name="resetSrc"
          control={control}
          items={resetSrcItems}
          label="Reset source"
          placeholder="Reset source"
        >
          {(item) => (
            <SelectItem
              startContent={
                (item as ResetSrcItems).icon ? (
                  <Icon
                    className={classNames(
                      "h-5 w-5",
                      (item as ResetSrcItems).iconClass,
                    )}
                    name={(item as ResetSrcItems).icon!}
                  />
                ) : undefined
              }
            >
              {item.value}
            </SelectItem>
          )}
        </ControlledSelect>
        <Controller
          name="internalBpm"
          control={control}
          render={({ field }) => (
            <Input
              {...inputProps}
              label="Internal BPM"
              type="number"
              inputMode="decimal"
              min={45.0}
              max={300.0}
              step="any"
              value={String(field.value)}
              onChange={(e) => field.onChange(Number(e.target.value))}
              onBlur={field.onBlur}
            />
          )}
        />
      </div>
    </div>
  );
};
