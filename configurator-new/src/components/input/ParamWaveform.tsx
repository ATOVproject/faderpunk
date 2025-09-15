import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Waveform } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";

import { pascalToKebab } from "../../utils/utils.ts";
import { Icon } from "../Icon";
import { selectProps } from "./defaultProps.tsx";

interface Props {
  defaultValue: string;
  paramIndex: number;
  name: string;
  register: UseFormRegister<FieldValues>;
  variants: Waveform[];
}

type Item = { key: Waveform["tag"]; value: string; icon: string };

export const ParamWaveform = ({
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
        value: variant.tag,
        icon: pascalToKebab(variant.tag),
      })),
    [variants],
  );
  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Waveform-${paramIndex}`)}
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
