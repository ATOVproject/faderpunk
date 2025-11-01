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
import { Link } from "react-router-dom";

import { useStore } from "../store";
import { COLORS_CLASSES } from "../utils/class-helpers";
import { setLayout } from "../utils/config";
import { ModalMode, type AppLayout, type ModalConfig } from "../utils/types";
import {
  addAppToLayout,
  pascalToKebab,
  recalculateStartChannels,
} from "../utils/utils";
import { ButtonPrimary, ButtonSecondary } from "./Button";
import { Icon } from "./Icon";
import { Item } from "./Item";
import { SortableItem } from "./SortableItem";

interface Props {
  initialLayout: AppLayout;
  onSave: (layout: AppLayout) => void;
  onClose: () => void;
  modalConfig: ModalConfig;
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
  modalConfig,
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

  const handleClearAll = useCallback(() => {
    setItems((items) => {
      const newAppItem =
        newAppId !== null ? items.find(({ id }) => id === newAppId) : null;

      if (newAppItem) {
        // Create empty layout with the new app preserved
        const emptyLayout: AppLayout = Array.from({ length: 16 }, (_, i) => ({
          id: i < newAppItem.startChannel ? i : i + 100,
          app: null,
          startChannel: i,
        }));

        // Insert the new app at its position
        const channels = Number(newAppItem.app?.channels) || 1;
        emptyLayout.splice(newAppItem.startChannel, channels, newAppItem);

        return emptyLayout;
      }

      // No app being added, clear everything
      return Array.from({ length: 16 }, (_, i) => ({
        id: i,
        app: null,
        startChannel: i,
      }));
    });
    setDeletePopoverId(null);
  }, [newAppId]);

  const handleSave = useCallback(async () => {
    if (usbDevice) {
      await setLayout(usbDevice, layout);
      onSave(layout);
    }
  }, [usbDevice, layout, onSave]);

  const activeItem =
    activeId !== null && layout.find(({ id }) => id == activeId);
  const appToAdd =
    apps &&
    modalConfig.mode === ModalMode.AddApp &&
    modalConfig.appToAdd &&
    modalConfig.appToAdd >= 0
      ? apps.get(modalConfig.appToAdd)
      : undefined;

  useEffect(() => {
    if (!appToAdd || newAppId !== null) return;

    const { success, newLayout, newId } = addAppToLayout(layout, appToAdd);

    if (success) {
      setItems(newLayout);
      setNewAppId(newId);
    }
  }, [layout, newAppId, appToAdd]);

  const cantAddError = appToAdd && newAppId === null;

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
                  COLORS_CLASSES[appToAdd.color].bg,
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
              <div className="justify-self-end">
                <h3 className="text-yellow-fp text-sm font-bold uppercase">
                  Resources
                </h3>
                <div className="text-base underline">
                  <Link to={`/manual#app-${appToAdd.appId}`} target="fpmanual">
                    See app in manual
                  </Link>
                </div>
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
              <div className="relative">
                <GridBackground />
                <div className="mr-1.5 ml-1.5 grid min-h-12 grid-cols-16 gap-3">
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
          <div className="mt-18 flex justify-center">
            <ButtonSecondary className="text-red" onPress={handleClearAll}>
              <Icon name="trash" /> Clear All Apps
            </ButtonSecondary>
          </div>
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
