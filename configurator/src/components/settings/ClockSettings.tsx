import type { ClockSrc, ResetSrc } from "@atov/fp-config";
import { Input } from "@heroui/input";
import { Select, SelectItem } from "@heroui/select";
import { Tooltip } from "@heroui/tooltip";
import classNames from "classnames";
import { Controller, useFormContext, useWatch } from "react-hook-form";
import { Icon } from "../Icon";
import { inputProps, selectProps } from "../input/defaultProps";
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

// Integral ratios of the internal 24 PPQN clock: divisors are multiplied up,
// multiples divided down. Must stay in sync with the firmware's
// `effective_ppqn` and the validator in utils/validators.ts.
const extPpqnItems = [
  { key: "1", value: "1 PPQN (quarter notes)" },
  { key: "2", value: "2 PPQN (8th notes)" },
  { key: "3", value: "3 PPQN (8th triplets)" },
  { key: "4", value: "4 PPQN (16th notes)" },
  { key: "6", value: "6 PPQN (16th triplets)" },
  { key: "8", value: "8 PPQN (32nd notes)" },
  { key: "12", value: "12 PPQN (32nd triplets)" },
  { key: "24", value: "24 PPQN (MIDI / DIN sync)" },
  { key: "48", value: "48 PPQN" },
  { key: "96", value: "96 PPQN" },
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
  const clockSrc = useWatch({ control, name: "clockSrc" });
  const isAnalogClockSrc =
    clockSrc === "Atom" || clockSrc === "Meteor" || clockSrc === "Cube";

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
        <Controller
          name="extPpqn"
          control={control}
          render={({ field }) => (
            <Select
              {...selectProps}
              classNames={{
                ...selectProps.classNames,
                label: "font-medium pb-2 w-full",
              }}
              label={
                <div className="flex w-full items-center justify-between gap-1">
                  <span>External PPQN</span>
                  <Tooltip
                    content="Pulses per quarter note of the analog clock input. Below 24 the clock is multiplied up, above 24 divided down. MIDI clock is always 24 PPQN. Swing is bypassed unless set to 24."
                    showArrow={true}
                  >
                    <button type="button" className="cursor-help">
                      <Icon className="h-4 w-4" name="info" />
                    </button>
                  </Tooltip>
                </div>
              }
              placeholder="External PPQN"
              isDisabled={!isAnalogClockSrc}
              selectedKeys={[String(field.value)]}
              onSelectionChange={(keys) => {
                if (keys.currentKey) {
                  field.onChange(Number(keys.currentKey));
                }
              }}
              items={extPpqnItems}
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </Select>
          )}
        />
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
        <Controller
          name="swingAmount"
          control={control}
          render={({ field }) => (
            <Input
              {...inputProps}
              classNames={{
                ...inputProps.classNames,
                label: "font-medium w-full",
              }}
              label={
                <div className="flex w-full items-center justify-between gap-1">
                  <span>Swing</span>
                  <Tooltip
                    content="-35..+35, 0 = straight. Applied at the 16th-note level."
                    showArrow={true}
                  >
                    <button type="button" className="cursor-help">
                      <Icon className="h-4 w-4" name="info" />
                    </button>
                  </Tooltip>
                </div>
              }
              type="number"
              inputMode="numeric"
              min={-35}
              max={35}
              step={1}
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
