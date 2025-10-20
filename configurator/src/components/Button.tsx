import { Button, type ButtonProps } from "@heroui/button";
import classNames from "classnames";

export const ButtonPrimary = ({ className, ...props }: ButtonProps) => {
  return (
    <Button
      radius="sm"
      color="primary"
      className={classNames("px-8 py-2.5 text-sm font-semibold", className)}
      {...props}
    />
  );
};

export const ButtonSecondary = ({ className, ...props }: ButtonProps) => {
  return (
    <Button
      radius="sm"
      className={classNames(
        "bg-transparent px-8 py-2.5 text-sm font-semibold text-white",
        className,
      )}
      {...props}
    />
  );
};
