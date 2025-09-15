import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { Switch } from "@heroui/switch";

interface Props {
  defaultValue: boolean;
  name: string;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

export const ParamBool = ({
  defaultValue,
  name,
  paramIndex,
  register,
}: Props) => (
  <div className="flex w-40 items-start">
    <Switch
      defaultChecked={defaultValue}
      {...register(`param-bool-${paramIndex}`)}
      color="secondary"
      classNames={{
        base: "flex-col-reverse items-start justify-start w-full",
        label: "ms-0 mb-2 text-sm font-medium",
      }}
    >
      {name}
    </Switch>
  </div>
);
