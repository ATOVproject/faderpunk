import type { I2cMode } from "@atov/fp-config";
import { SelectItem } from "@heroui/select";
import { useFormContext } from "react-hook-form";

import type { Inputs } from "../SettingsTab";
import { ControlledSelect } from "./ControlledFields";

interface I2cModeItem {
  key: I2cMode["tag"];
  value: string;
}

const i2cModeItems: I2cModeItem[] = [
  { key: "Follower", value: "Follower" },
  { key: "Leader", value: "Leader" },
];

export const I2cSettings = () => {
  const { control } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">I²C</h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <ControlledSelect
          name="i2cMode"
          control={control}
          items={i2cModeItems}
          label="I²C mode"
          placeholder="I²C mode"
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </ControlledSelect>
      </div>
    </div>
  );
};
