import type { latch } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";
import { Slider } from "@heroui/slider";
import { Tooltip } from "@heroui/tooltip";
import { useCallback } from "react";
import { useFormContext } from "react-hook-form";
import { Icon } from "../Icon";
import { selectProps } from "../input/defaultProps";
import type { Inputs } from "../SettingsTab";

interface TakeoverModeItem {
  key: latch.TakeoverMode["tag"];
  value: string;
  description: string;
}

const takeoverModeItems: TakeoverModeItem[] = [
  {
    key: "Pickup",
    value: "Pickup (Default)",
    description: "Wait until fader crosses target value",
  },
  {
    key: "Jump",
    value: "Jump",
    description: "Immediate takeover, no pickup delay",
  },
  {
    key: "Scale",
    value: "Scale",
    description: "Gradual convergence to fader position",
  },
];

export const MiscSettings = () => {
  const { register, getValues } = useFormContext<Inputs>();

  const {
    onChange: onChangeBrightness,
    onBlur: onBlurBrightness,
    name: nameBrightness,
    ref: refBrightness,
  } = register("ledBrightness", {
    valueAsNumber: true,
  });

  const handleChangeBrightness = useCallback(
    (value: number | number[]) => {
      if (!Array.isArray(value)) {
        onChangeBrightness({ target: { name: nameBrightness, value } });
      }
    },
    [nameBrightness, onChangeBrightness],
  );

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Miscellaneous
      </h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <Slider
          defaultValue={getValues("ledBrightness")}
          minValue={100}
          maxValue={255}
          onBlur={onBlurBrightness}
          name={nameBrightness}
          onChange={handleChangeBrightness}
          label="LED Brightness"
          ref={refBrightness}
        />
        <Select
          {...register("takeoverMode")}
          {...selectProps}
          classNames={{
            ...selectProps.classNames,
            label: "font-medium pb-2 w-full",
          }}
          label={
            <div className="flex w-full items-center justify-between gap-1">
              <span>Fader Takeover Mode</span>
              <Tooltip
                content="How faders take control when switching layers"
                showArrow={true}
              >
                <button type="button" className="cursor-help">
                  <Icon className="h-4 w-4" name="info" />
                </button>
              </Tooltip>
            </div>
          }
          items={takeoverModeItems}
          placeholder="Select mode"
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </Select>
      </div>
    </div>
  );
};
