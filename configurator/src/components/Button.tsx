import { Button, type ButtonProps } from "@heroui/button";

export const ButtonPrimary = (props: ButtonProps) => {
  return (
    <Button
      radius="sm"
      color="primary"
      className="px-8 py-2.5 text-sm font-semibold"
      {...props}
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
