import { useMemo } from "react";
import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Note } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "./defaultProps";

interface Props {
  defaultValue: string;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
  variants: Note[];
}

type Item = { key: Note["tag"]; value: string };

export const ParamNote = ({
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
        value: variant.tag.replace("Sharp", "♯"),
      })),
    [variants],
  );
  return (
    <Select
      defaultSelectedKeys={[defaultValue]}
      {...register(`param-Note-${paramIndex}`)}
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
