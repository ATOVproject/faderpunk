import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

type Item = { key: string; value: string };

const ITEMS: Item[] = [
  { key: "Standard", value: "1V/Oct (Eurorack)" },
  { key: "Buchla", value: "1.2V/Oct (Buchla)" },
  { key: "Custom:0", value: "Custom 1" },
  { key: "Custom:1", value: "Custom 2" },
  { key: "Custom:2", value: "Custom 3" },
  { key: "Custom:3", value: "Custom 4" },
];

export const ParamVoltPerOct = ({
  defaultValue,
  paramIndex,
  register,
}: Props) => {
  const items = useMemo(() => ITEMS, []);

  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-VoltPerOct-${paramIndex}`)}
      {...selectProps}
      label="V/Oct Standard"
      items={items}
      placeholder="V/Oct Standard"
    >
      {(item: Item) => <SelectItem key={item.key}>{item.value}</SelectItem>}
    </Select>
  );
};
