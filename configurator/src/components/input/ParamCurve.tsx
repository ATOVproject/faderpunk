import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Curve } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";

import { Icon } from "../Icon";
import { selectProps } from "./defaultProps.tsx";
import { useMemo } from "react";
import { pascalToKebab } from "../../utils/utils.ts";

interface Props {
  defaultValue: string;
  paramIndex: number;
  name: string;
  register: UseFormRegister<FieldValues>;
  variants: Curve[];
}

type Item = { key: Curve["tag"]; value: string; icon: string };

export const ParamCurve = ({
  defaultValue,
  name,
  paramIndex,
  register,
  variants,
}: Props) => {
  const items = useMemo(
    () =>
      variants.map((variant) => ({
        key: variant.tag,
        value: variant.tag,
        icon: pascalToKebab(variant.tag),
      })),
    [variants],
  );
  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Curve-${paramIndex}`)}
      {...selectProps}
      label={name}
      items={items}
      placeholder={name}
    >
      {(item: Item) => (
        <SelectItem
          className="text-white"
          startContent={<Icon name={item.icon} />}
        >
          {item.value}
        </SelectItem>
      )}
    </Select>
  );
};
