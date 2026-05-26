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
      {(item: Item) => <SelectItem>{item.value}</SelectItem>}
    </Select>
  );
};
