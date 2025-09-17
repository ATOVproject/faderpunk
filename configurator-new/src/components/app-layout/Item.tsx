import classNames from "classnames";
import { forwardRef, type ForwardedRef, type ComponentProps } from "react";
import { Icon } from "../Icon";
import type { AppSlot } from "../../utils/types";
import { COLORS_CLASSES, WIDTHS_CLASSES } from "../../utils/class-helpers";
import { pascalToKebab } from "../../utils/utils";

interface Props extends ComponentProps<"div"> {
  item: AppSlot;
}

export const Item = forwardRef(
  ({ item, className, ...props }: Props, ref: ForwardedRef<HTMLDivElement>) => {
    if ("slotNumber" in item) {
      return (
        <div
          className={classNames("grow-1 outline-none", className)}
          {...props}
          ref={ref}
        >
          <span className="h8" />
        </div>
      );
    }

    return (
      <div
        className={classNames(
          "z-10 flex cursor-grab touch-manipulation justify-center rounded-sm p-2 whitespace-nowrap outline-none",
          className,
          COLORS_CLASSES[item.color],
          WIDTHS_CLASSES[Number(item.channels)],
        )}
        {...props}
        ref={ref}
      >
        <Icon className="h-8 w-8 text-black" name={pascalToKebab(item.icon)} />
      </div>
    );
  },
);
