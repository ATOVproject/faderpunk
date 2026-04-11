import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Item } from "./Item";
import type { AppSlot } from "../utils/types";
import type { Dispatch, SetStateAction } from "react";

interface Props {
  canDuplicate?: boolean;
  deletePopoverId: number | null;
  item: AppSlot;
  newAppId?: number;
  onDeleteItem(itemId: number): void;
  onDuplicateItem?(itemId: number): void;
  setDeletePopoverId: Dispatch<SetStateAction<number | null>>;
}

export const SortableItem = ({
  canDuplicate,
  item,
  onDeleteItem,
  onDuplicateItem,
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
      canDuplicate={canDuplicate}
      isDragging={isDragging}
      item={item}
      newAppId={newAppId}
      onDeleteItem={onDeleteItem}
      onDuplicateItem={onDuplicateItem}
      deletePopoverId={deletePopoverId}
      setDeletePopoverId={setDeletePopoverId}
      {...attributes}
      {...listeners}
    />
  );
};
