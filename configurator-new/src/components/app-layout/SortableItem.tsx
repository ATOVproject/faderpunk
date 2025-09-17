import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Item } from "./Item";
import type { AppSlot } from "../../utils/types";

interface Props {
  item: AppSlot;
}

export const SortableItem = ({ item }: Props) => {
  const { attributes, listeners, setNodeRef, transform, transition } =
    useSortable({ id: item.id, disabled: "slotNumber" in item });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <Item
      ref={setNodeRef}
      style={style}
      item={item}
      {...attributes}
      {...listeners}
    />
  );
};
