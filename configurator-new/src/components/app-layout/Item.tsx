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
    if (!item.app) {
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

    const { app } = item;
    return (
      <div
        className={classNames(
          "z-10 flex cursor-grab touch-manipulation justify-center rounded-sm p-2 whitespace-nowrap outline-none",
          className,
          COLORS_CLASSES[app.color],
          WIDTHS_CLASSES[Number(app.channels)],
        )}
        {...props}
        ref={ref}
      >
        <Icon className="h-8 w-8 text-black" name={pascalToKebab(app.icon)} />
      </div>
    );
  },
);
