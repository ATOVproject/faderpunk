import classNames from "classnames";
import {
  forwardRef,
  type ForwardedRef,
  type ComponentProps,
  type Dispatch,
  type SetStateAction,
  useCallback,
  useState,
} from "react";
import { Tooltip } from "@heroui/tooltip";

import { Icon } from "../Icon";
import type { AppSlot } from "../../utils/types";
import { COLORS_CLASSES, WIDTHS_CLASSES } from "../../utils/class-helpers";
import { pascalToKebab } from "../../utils/utils";

interface DeleteTooltipProps {
  handleDeleteItem(): void;
}

const DeletePopover = ({ handleDeleteItem }: DeleteTooltipProps) => (
  <button
    className="flex cursor-pointer items-center justify-center gap-2"
    onClick={handleDeleteItem}
  >
    <Icon className="text-red h-3 w-3" name="trash" />
    <span className="text-xs font-medium">Delete</span>
  </button>
);

interface Props extends ComponentProps<"div"> {
  deletePopoverId: number | null;
  isDragging?: boolean;
  item: AppSlot;
  newAppId: number | null;
  onDeleteItem(itemId: number): void;
  setDeletePopoverId: Dispatch<SetStateAction<number | null>>;
}

export const Item = forwardRef(
  (
    {
      className,
      isDragging,
      item,
      onDeleteItem,
      newAppId,
      deletePopoverId,
      setDeletePopoverId,
      ...props
    }: Props,
    ref: ForwardedRef<HTMLDivElement>,
  ) => {
    const [isHovered, setIsHovered] = useState(false);

    const handleClick = useCallback(() => {
      if (item.id === deletePopoverId) {
        setDeletePopoverId(null);
      } else {
        setDeletePopoverId(item.id);
      }
    }, [deletePopoverId, setDeletePopoverId, item.id]);

    const handleDeleteItem = useCallback(() => {
      onDeleteItem(item.id);
    }, [onDeleteItem, item.id]);

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

    const { app, id } = item;

    const showDeletePopover = deletePopoverId === id && newAppId !== id;

    return (
      <Tooltip
        className="bg-default-100"
        classNames={{
          base: "before:bg-default-100",
        }}
        radius="sm"
        content={
          showDeletePopover ? (
            <DeletePopover handleDeleteItem={handleDeleteItem} />
          ) : (
            <span className="text-xs font-medium">{app.name}</span>
          )
        }
        showArrow={true}
        isOpen={!isDragging && (isHovered || showDeletePopover)}
      >
        <div
          className={classNames(
            "z-10 flex cursor-grab touch-manipulation justify-center rounded-sm p-2 whitespace-nowrap outline-none",
            className,
            COLORS_CLASSES[app.color],
            WIDTHS_CLASSES[Number(app.channels)],
            {
              "shadow-[0px_0px_16px_2px_#FFFFFFCC]": newAppId === id,
            },
          )}
          {...props}
          onClick={handleClick}
          onMouseEnter={() => setIsHovered(true)}
          onMouseLeave={() => setIsHovered(false)}
          ref={ref}
        >
          <Icon className="h-8 w-8 text-black" name={pascalToKebab(app.icon)} />
        </div>
      </Tooltip>
    );
  },
);
