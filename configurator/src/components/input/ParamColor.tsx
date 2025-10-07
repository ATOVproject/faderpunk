import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Color } from "@atov/fp-config";

import { Select, SelectItem } from "@heroui/select";
import { selectProps } from "./defaultProps.tsx";
import { useMemo } from "react";
import classNames from "classnames";
import { COLORS_CLASSES } from "../../utils/class-helpers.ts";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
  variants: Color[];
}

type Item = { key: Color["tag"]; value: Color["tag"] };

export const ParamColor = ({
  defaultValue,
  name,
  paramIndex,
  register,
  variants,
}: Props) => {
  const items = useMemo(
    () => variants.map((variant) => ({ key: variant.tag, value: variant.tag })),
    [variants],
  );
  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Color-${paramIndex}`)}
      {...selectProps}
      label={name}
      items={items}
      placeholder={name}
    >
      {(item: Item) => (
        <SelectItem
          startContent={
            <span
              className={classNames("h-5", "w-5", COLORS_CLASSES[item.key].bg)}
            />
          }
        >
          {item.value}
        </SelectItem>
      )}
    </Select>
  );
};
