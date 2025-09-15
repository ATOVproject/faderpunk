import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Range } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
  variants: Range[];
}

type Item = { key: Range["tag"]; value: string };

const getValue = (key: Range["tag"]) => {
  switch (key) {
    case "_0_10V": {
      return "0V - 10V";
    }
    case "_0_5V": {
      return "0V - 5V";
    }
    case "_Neg5_5V": {
      return "-5V - 5V";
    }
  }
};

export const ParamRange = ({
  defaultValue,
  paramIndex,
  name,
  register,
  variants,
}: Props) => {
  const items = useMemo(
    () =>
      variants.map((variant) => ({
        key: variant.tag,
        value: getValue(variant.tag),
      })),
    [variants],
  );
  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Range-${paramIndex}`)}
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
