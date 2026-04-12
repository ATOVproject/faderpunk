import type { latch } from "@atov/fp-config";
import { SelectItem } from "@heroui/select";
import { Tooltip } from "@heroui/tooltip";
import { useFormContext } from "react-hook-form";
import { Icon } from "../Icon";
import type { Inputs } from "../SettingsTab";
import { ControlledSelect, ControlledSlider } from "./ControlledFields";

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
  const { control } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Miscellaneous
      </h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <ControlledSlider
          name="ledBrightness"
          control={control}
          label="LED Brightness"
          minValue={100}
          maxValue={255}
        />
        <ControlledSelect
          name="takeoverMode"
          control={control}
          items={takeoverModeItems}
          placeholder="Select mode"
          selectProps={{
            classNames: {
              base: "flex-col items-start",
              label: "font-medium pb-2 w-full",
              popoverContent: "rounded-xs",
            },
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
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </ControlledSelect>
      </div>
    </div>
  );
};
