import { Button, type ButtonProps } from "@heroui/button";
import classNames from "classnames";

export const ButtonPrimary = (props: ButtonProps) => {
  return (
    <Button
      radius="sm"
      color="primary"
      {...props}
      className={classNames(
        "px-8 py-2.5 text-sm font-semibold",
        props.className,
      )}
    />
  );
};

export const ButtonSecondary = (props: ButtonProps) => {
  return (
    <Button
      radius="sm"
      className="bg-transparent px-8 py-2.5 text-sm font-semibold text-white"
      {...props}
    />
  );
};
