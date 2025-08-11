import { Switch } from "@heroui/switch";
import { ComponentProps } from "react";

export function BoolParamInput(props: ComponentProps<typeof Switch>) {
  return <Switch {...props} />;
}
