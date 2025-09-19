import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Item } from "./Item";
import type { AppSlot } from "../../utils/types";
import type { Dispatch, SetStateAction } from "react";

interface Props {
  item: AppSlot;
  deletePopoverId: number | null;
  onDeleteItem(itemId: number): void;
  setDeletePopoverId: Dispatch<SetStateAction<number | null>>;
}

export const SortableItem = ({
  item,
  onDeleteItem,
  deletePopoverId,
  setDeletePopoverId,
}: Props) => {
  const { attributes, listeners, setNodeRef, transform, transition } =
    useSortable({ id: item.id, disabled: !item.app });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <Item
      ref={setNodeRef}
      style={style}
      item={item}
      onDeleteItem={onDeleteItem}
      deletePopoverId={deletePopoverId}
      setDeletePopoverId={setDeletePopoverId}
      {...attributes}
      {...listeners}
    />
  );
};
