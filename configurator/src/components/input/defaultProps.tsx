import { type InputProps } from "@heroui/input";
import { type SelectProps } from "@heroui/select";

import { Icon } from "../Icon";

export const selectProps = {
  className: "max-w-48",
  labelPlacement: "outside-left",
  disallowEmptySelection: true,
  radius: "sm",
  classNames: {
    base: "flex-col items-start",
    label: "font-medium pb-2",
    popoverContent: "rounded-xs",
  },
  selectorIcon: (
    <span>
      <Icon className="h-4 w-4 rotate-90" name="caret" />
    </span>
  ),
} as SelectProps;

export const inputProps = {
  className: "max-w-48",
  labelPlacement: "outside-top",
  disableAnimation: true,
  classNames: {
    label: "font-medium",
  },
  radius: "sm",
} as InputProps;
