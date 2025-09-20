import type { I2cMode } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";
import { useFormContext } from "react-hook-form";

import { selectProps } from "../input/defaultProps";
import type { Inputs } from "../SettingsTab";

interface I2cModeItem {
  key: I2cMode["tag"];
  value: string;
}

const i2cModeItems: I2cModeItem[] = [
  { key: "Follower", value: "Follower" },
  { key: "Leader", value: "Leader" },
];

export const I2cSettings = () => {
  const { register } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">I²C</h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <Select
          {...register("i2cMode")}
          {...selectProps}
          label="I²C mode"
          items={i2cModeItems}
          placeholder="I²C mode"
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </Select>
      </div>
    </div>
  );
};
