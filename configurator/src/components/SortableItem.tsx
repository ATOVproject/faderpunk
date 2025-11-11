import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Item } from "./Item";
import type { AppSlot } from "../utils/types";
import type { Dispatch, SetStateAction } from "react";

interface Props {
  deletePopoverId: number | null;
  item: AppSlot;
  newAppId?: number;
  onDeleteItem(itemId: number): void;
  setDeletePopoverId: Dispatch<SetStateAction<number | null>>;
}

export const SortableItem = ({
  item,
  onDeleteItem,
  deletePopoverId,
  newAppId,
  setDeletePopoverId,
}: Props) => {
  const {
    attributes,
    isDragging,
    listeners,
    setNodeRef,
    transform,
    transition,
  } = useSortable({ id: item.id, disabled: !item.app });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <Item
      ref={setNodeRef}
      style={style}
      isDragging={isDragging}
      item={item}
      newAppId={newAppId}
      onDeleteItem={onDeleteItem}
      deletePopoverId={deletePopoverId}
      setDeletePopoverId={setDeletePopoverId}
      {...attributes}
      {...listeners}
    />
  );
};
