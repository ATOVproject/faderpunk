import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
  variants: string[];
}

type Item = { key: number; value: string };

export const ParamEnum = ({
  defaultValue,
  name,
  paramIndex,
  register,
  variants,
}: Props) => {
  const items = useMemo(
    () => variants.map((variant, idx) => ({ key: idx, value: variant })),
    [variants],
  );

  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Enum-${paramIndex}`)}
      {...selectProps}
      label={name}
      items={items}
      placeholder={name}
    >
      {(item: Item) => (
        <SelectItem className="text-white">{item.value}</SelectItem>
      )}
    </Select>
  );
};
