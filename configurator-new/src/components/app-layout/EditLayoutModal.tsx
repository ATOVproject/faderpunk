import { useCallback, useState } from "react";
import {
  closestCenter,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
  type UniqueIdentifier,
} from "@dnd-kit/core";
import {
  arrayMove,
  horizontalListSortingStrategy,
  SortableContext,
  sortableKeyboardCoordinates,
} from "@dnd-kit/sortable";

import { SortableItem } from "./SortableItem";
import { Item } from "./Item";
import type { AppLayout } from "../../utils/types";
import { Button, ModalBody, ModalFooter, ModalHeader } from "@heroui/react";
import { ButtonPrimary, ButtonSecondary } from "../Button";
import { Icon } from "../Icon";
import { setLayout } from "../../utils/config";
import { useStore } from "../../store";

interface Props {
  initialLayout: AppLayout;
  onSave: (layout: AppLayout) => void;
  onClose: () => void;
}

const GridBackground = () => {
  const gridArray = Array.from({ length: 16 }, (_, index) => index);

  return (
    <div className="absolute grid h-[110%] w-full grid-cols-16">
      {gridArray.map((item) => (
        <div
          key={item}
          className="border-default-100 border-r-1.5 border-l-1.5 flex translate-y-8 items-end justify-center text-lg font-bold select-none first:border-l-3 last:border-r-3"
        >
          {item + 1}
        </div>
      ))}
    </div>
  );
};

export const EditLayoutModal = ({ initialLayout, onSave, onClose }: Props) => {
  const { usbDevice } = useStore();
  const [activeId, setActiveId] = useState<UniqueIdentifier | null>(null);
  const [layout, setItems] = useState<AppLayout>(initialLayout);
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleDragStart = useCallback((event: DragStartEvent) => {
    const { active } = event;
    setActiveId(active.id);
  }, []);

  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;

    if (active.id !== over?.id) {
      setItems((items) => {
        const oldIndex = items.findIndex(({ id }) => active.id === id);
        const newIndex = items.findIndex(({ id }) => over?.id === id);

        const reorderedItems = arrayMove(items, oldIndex, newIndex);

        let runningChannel = 0;
        const finalItems = reorderedItems.map((item) => {
          const newItem = { ...item, startChannel: runningChannel };
          // The next item's start channel is the current one's start plus its channel count
          runningChannel += Number(item.app?.channels) || 1;
          return newItem;
        });

        return finalItems;
      });
    }
    setActiveId(null);
  }, []);

  const handleSave = useCallback(async () => {
    if (usbDevice) {
      await setLayout(usbDevice, layout);
      onSave(layout);
    }
  }, [usbDevice, layout, onSave]);

  const activeItem = !!activeId && layout.find(({ id }) => id == activeId);

  return (
    <>
      <ModalHeader className="px-10 pt-10 pb-0">
        <div className="flex w-full justify-between">
          <span className="text-yellow-fp text-lg font-bold uppercase">
            Edit Layout
          </span>
          <Button
            isIconOnly
            className="cursor-pointer bg-transparent"
            onPress={onClose}
          >
            <Icon name="xmark" />
          </Button>
        </div>
      </ModalHeader>
      <ModalBody className="px-10">
        <div className="border-default-100 border-t-3 border-b-3 py-10">
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragEnd={handleDragEnd}
          >
            <SortableContext
              items={layout}
              strategy={horizontalListSortingStrategy}
            >
              <div className="relative mb-10">
                <GridBackground />
                <div className="mr-1.5 ml-1.5 grid grid-cols-16 gap-3">
                  {layout
                    .filter((item) => item.app !== undefined)
                    .map((item) => (
                      <SortableItem item={item} key={item.id} />
                    ))}
                </div>
              </div>
            </SortableContext>
            <DragOverlay>
              {activeItem ? (
                <Item className="opacity-60 shadow-md" item={activeItem} />
              ) : null}
            </DragOverlay>
          </DndContext>
        </div>
      </ModalBody>
      <ModalFooter>
        <ButtonPrimary
          onPress={() => {
            handleSave();
            onClose();
          }}
        >
          Save
        </ButtonPrimary>
        <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
      </ModalFooter>
    </>
  );
};
