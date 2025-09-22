import type { ClockSrc, ResetSrc } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";
import { Input } from "@heroui/input";
import { useFormContext } from "react-hook-form";
import classNames from "classnames";

import { inputProps, selectProps } from "../input/defaultProps";
import { Icon } from "../Icon";
import type { Inputs } from "../SettingsTab";

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
  const { register } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">Clock</h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <Select
          {...register("clockSrc")}
          {...selectProps}
          label="Clock source"
          items={clockSrcItems}
          placeholder="Clock source"
        >
          {(item) => (
            <SelectItem
              startContent={
                item.icon ? (
                  <Icon
                    className={classNames("h-5 w-5", item.iconClass)}
                    name={item.icon}
                  />
                ) : undefined
              }
            >
              {item.value}
            </SelectItem>
          )}
        </Select>
        <Select
          {...register("resetSrc")}
          {...selectProps}
          label="Reset source"
          items={resetSrcItems}
          placeholder="Reset source"
        >
          {(item) => (
            <SelectItem
              startContent={
                item.icon ? (
                  <Icon
                    className={classNames("h-5 w-5", item.iconClass)}
                    name={item.icon}
                  />
                ) : undefined
              }
            >
              {item.value}
            </SelectItem>
          )}
        </Select>
        <Input
          {...register("internalBpm", { valueAsNumber: true })}
          {...inputProps}
          label="Internal BPM"
          type="number"
          inputMode="decimal"
          min={45.0}
          max={300.0}
          step="any"
        />
      </div>
    </div>
  );
};
