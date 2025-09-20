import { useCallback, useEffect, useState } from "react";
import classNames from "classnames";
import { Button } from "@heroui/button";
import { ModalBody, ModalFooter, ModalHeader } from "@heroui/modal";
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
import { ButtonPrimary, ButtonSecondary } from "../Button";
import { Icon } from "../Icon";
import { setLayout } from "../../utils/config";
import { useStore } from "../../store";
import { COLORS_CLASSES } from "../../utils/class-helpers";
import {
  addAppToLayout,
  pascalToKebab,
  recalculateStartChannels,
} from "../../utils/utils";

interface Props {
  initialLayout: AppLayout;
  onSave: (layout: AppLayout) => void;
  onClose: () => void;
  modalApp: number | null;
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

export const EditLayoutModal = ({
  initialLayout,
  onSave,
  onClose,
  modalApp,
}: Props) => {
  const { usbDevice, apps } = useStore();
  const [activeId, setActiveId] = useState<UniqueIdentifier | null>(null);
  const [layout, setItems] = useState<AppLayout>(initialLayout);
  const [newAppId, setNewAppId] = useState<number | null>(null);
  const [deletePopoverId, setDeletePopoverId] = useState<number | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleDragStart = useCallback((event: DragStartEvent) => {
    const { active } = event;
    setActiveId(active.id);
    setDeletePopoverId(null);
  }, []);

  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;

    if (active.id !== over?.id) {
      setItems((items) => {
        const oldIndex = items.findIndex(({ id }) => active.id === id);
        const newIndex = items.findIndex(({ id }) => over?.id === id);

        const reorderedItems = arrayMove(items, oldIndex, newIndex);

        return recalculateStartChannels(reorderedItems);
      });
    }
    setActiveId(null);
  }, []);

  const handleDeleteItem = useCallback((idToDelete: number) => {
    setItems((items) => {
      const itemToDeleteIndex = items.findIndex(({ id }) => id === idToDelete);
      if (itemToDeleteIndex === -1) {
        return items;
      }

      const itemToDelete = items[itemToDeleteIndex];
      const channelsToDelete = Number(itemToDelete.app?.channels) || 1;
      const startChannelOfDeleted = itemToDelete.startChannel;

      const emptySlotIds = items
        .filter((item) => !item.app)
        .map((item) => item.id);
      const lastId = emptySlotIds.length > 0 ? Math.max(...emptySlotIds) : 15;
      let nextId = lastId + 1;

      const newEmptySlots = Array.from(
        { length: channelsToDelete },
        (_, i) => ({
          id: nextId++,
          app: null,
          startChannel: startChannelOfDeleted + i,
        }),
      );

      const finalItems = [...items];
      finalItems.splice(itemToDeleteIndex, 1, ...newEmptySlots);

      return finalItems;
    });
    setDeletePopoverId(null);
  }, []);

  const handleSave = useCallback(async () => {
    if (usbDevice) {
      await setLayout(usbDevice, layout);
      onSave(layout);
    }
  }, [usbDevice, layout, onSave]);

  const activeItem = !!activeId && layout.find(({ id }) => id == activeId);
  const appToAdd =
    apps && modalApp && modalApp >= 0 ? apps.get(modalApp) : undefined;

  useEffect(() => {
    if (!appToAdd || modalApp === null || newAppId) return;

    const { success, newLayout, newId } = addAppToLayout(layout, appToAdd);

    if (success) {
      setItems(newLayout);
      setNewAppId(newId);
    }
  }, [layout, newAppId, appToAdd, modalApp]);

  const cantAddError = appToAdd && !newAppId;

  return (
    <>
      <ModalHeader className="px-10 pt-10 pb-0">
        <div className="flex w-full justify-between">
          <span className="text-yellow-fp text-lg font-bold uppercase">
            {appToAdd ? "Add App" : "Edit Layout"}
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
          {appToAdd ? (
            <div className="mb-12 flex items-start gap-x-4">
              <div
                className={classNames(
                  "rounded-sm p-2",
                  COLORS_CLASSES[appToAdd.color],
                )}
              >
                <Icon
                  className="h-12 w-12 text-black"
                  name={pascalToKebab(appToAdd.icon)}
                />
              </div>
              <div className="flex-1">
                <h3 className="text-yellow-fp text-sm font-bold uppercase">
                  App
                </h3>
                <div className="text-lg font-bold">{appToAdd.name}</div>
                <div className="text-sm font-medium">
                  {appToAdd.description}
                </div>
              </div>
              <div
                className={classNames({
                  "flex-1": appToAdd.paramCount <= 4,
                  "flex-2": appToAdd.paramCount > 4,
                })}
              >
                <h3 className="text-yellow-fp text-sm font-bold uppercase">
                  Parameters
                </h3>
                <ul
                  className={classNames("grid text-base/8", {
                    "grid-cols-1": appToAdd.paramCount <= 4,
                    "grid-cols-2": appToAdd.paramCount > 4,
                  })}
                >
                  {appToAdd.params.map((param, idx) => (
                    <li key={idx}>
                      {param.tag !== "None" && param.value.name}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="flex-1">
                <h3 className="text-yellow-fp text-sm font-bold uppercase">
                  Channels
                </h3>
                <div className="text-base">{Number(appToAdd.channels)}</div>
              </div>
            </div>
          ) : null}
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
                  {layout.map((item) => (
                    <SortableItem
                      onDeleteItem={handleDeleteItem}
                      deletePopoverId={deletePopoverId}
                      setDeletePopoverId={setDeletePopoverId}
                      newAppId={newAppId}
                      item={item}
                      key={item.id}
                    />
                  ))}
                </div>
              </div>
            </SortableContext>
            <DragOverlay>
              {activeItem ? (
                <Item
                  className="opacity-60 shadow-md"
                  onDeleteItem={handleDeleteItem}
                  deletePopoverId={deletePopoverId}
                  newAppId={newAppId}
                  isDragging={true}
                  setDeletePopoverId={setDeletePopoverId}
                  item={activeItem}
                />
              ) : null}
            </DragOverlay>
          </DndContext>
        </div>
      </ModalBody>
      <ModalFooter className="flex justify-between px-10">
        {cantAddError && (
          <span className="text-danger">
            I can't find space for the app. Try to remove apps or move them
            around.
          </span>
        )}
        <span className="ml-auto">
          <ButtonPrimary
            isDisabled={cantAddError}
            onPress={() => {
              handleSave();
              onClose();
            }}
          >
            Save
          </ButtonPrimary>
          <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
        </span>
      </ModalFooter>
    </>
  );
};
