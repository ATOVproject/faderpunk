import { useCallback, useEffect, useMemo, useState } from "react";
import classNames from "classnames";
import { Button } from "@heroui/button";
import { ModalBody, ModalFooter, ModalHeader } from "@heroui/modal";
import { Switch } from "@heroui/switch";
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
import {
  getAllAppParams,
  getAppParams,
  setAllAppParams,
  setGlobalConfig,
  setLayout,
} from "../utils/config";
import {
  App,
  ModalMode,
  type AppLayout,
  type ModalConfig,
} from "../utils/types";
import {
  addAppToLayout,
  delay,
  pascalToKebab,
  recalculateStartChannels,
} from "../utils/utils";
import { ButtonPrimary, ButtonSecondary } from "./Button";
import { Icon } from "./Icon";
import { Item } from "./Item";
import { SortableItem } from "./SortableItem";

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

interface NewAppDetailsProps {
  app: App;
}

const NewAppDetails = ({ app }: NewAppDetailsProps) => (
  <div className="mb-12 flex items-start gap-x-4">
    <div className={classNames("rounded-sm p-2", COLORS_CLASSES[app.color].bg)}>
      <Icon className="h-12 w-12 text-black" name={pascalToKebab(app.icon)} />
    </div>
    <div className="flex-1">
      <h3 className="text-yellow-fp text-sm font-bold uppercase">App</h3>
      <div className="text-lg font-bold">{app.name}</div>
      <div className="text-sm font-medium">{app.description}</div>
    </div>
    <div
      className={classNames({
        "flex-1": app.paramCount <= 4,
        "flex-2": app.paramCount > 4,
      })}
    >
      <h3 className="text-yellow-fp text-sm font-bold uppercase">Parameters</h3>
      <ul
        className={classNames("grid text-base/8", {
          "grid-cols-1": app.paramCount <= 4,
          "grid-cols-2": app.paramCount > 4,
        })}
      >
        {app.params.map((param, idx) => (
          <li key={idx}>{param.tag !== "None" && param.value.name}</li>
        ))}
      </ul>
    </div>
    <div className="flex-1">
      <h3 className="text-yellow-fp text-sm font-bold uppercase">Channels</h3>
      <div className="text-base">{Number(app.channels)}</div>
    </div>
    <div className="justify-self-end">
      <h3 className="text-yellow-fp text-sm font-bold uppercase">Resources</h3>
      <div className="text-base underline">
        <Link to={`/manual#app-${app.appId}`} target="fpmanual">
          See app in manual
        </Link>
      </div>
    </div>
  </div>
);

interface Props {
  initialLayout: AppLayout;
  onSave: (layout: AppLayout) => void;
  onClose: () => void;
  modalConfig: ModalConfig;
}

export const EditLayoutModal = ({
  initialLayout,
  onSave,
  onClose,
  modalConfig,
}: Props) => {
  const { usbDevice, apps, setParams, setAllParams, setConfig } = useStore();
  const [activeId, setActiveId] = useState<UniqueIdentifier | null>(null);
  const [layout, setItems] = useState<AppLayout>(initialLayout);
  const [newApp, setNewApp] = useState<App | null>(null);
  const [newAppId, setNewAppId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recallParams, setRecallParams] = useState<boolean>(true);
  const [recallConfig, setRecallConfig] = useState<boolean>(true);
  const [deletePopoverId, setDeletePopoverId] = useState<number | null>(null);
  const [isSubmitting, setSubmitting] = useState(false);

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
    setSubmitting(true);
    if (usbDevice && apps) {
      const newLayout = await setLayout(usbDevice, layout, apps);
      if (modalConfig.mode === ModalMode.RecallSetup) {
        if (recallParams && modalConfig.recallParams) {
          await setAllAppParams(usbDevice, modalConfig.recallParams);
          // Wait 1s for the apps to spawn
          await delay(1000);
          const params = await getAllAppParams(usbDevice);
          setAllParams(params);
        }
        if (recallConfig && modalConfig.recallConfig) {
          await setGlobalConfig(usbDevice, modalConfig.recallConfig);
          setConfig(modalConfig.recallConfig);
        }
      } else if (modalConfig.mode === ModalMode.AddApp && newAppId !== null) {
        // Wait 500ms for the new app to spawn
        await delay(500);
        const params = await getAppParams(usbDevice, newAppId);
        setParams(newAppId, params);
      }
      onSave(newLayout);
    }
    setSubmitting(false);
  }, [
    apps,
    usbDevice,
    layout,
    modalConfig.recallParams,
    modalConfig.mode,
    modalConfig.recallConfig,
    newAppId,
    onSave,
    recallParams,
    recallConfig,
    setParams,
    setAllParams,
    setConfig,
  ]);

  const activeItem =
    activeId !== null && layout.find(({ id }) => id == activeId);

  useEffect(() => {
    if (
      // wrong mode
      modalConfig.mode !== ModalMode.AddApp ||
      // app to add is missing
      !modalConfig.appToAdd ||
      // new app already placed
      newAppId !== null
    ) {
      return;
    }

    const appToAdd =
      apps && modalConfig.appToAdd ? apps.get(modalConfig.appToAdd) : undefined;
    if (!appToAdd) {
      return;
    }

    setNewApp(appToAdd);

    const { success, newLayout, newId } = addAppToLayout(layout, appToAdd);

    if (success && newId !== null) {
      setItems(newLayout);
      setNewAppId(newId);
      setError(null);
    } else {
      setError(
        "I can't find space for the app. Try to remove apps or move them around.",
      );
    }
  }, [layout, newApp, newAppId, apps, modalConfig]);

  const modalTitle = useMemo(() => {
    switch (modalConfig.mode) {
      case ModalMode.AddApp:
        return "Add App";
      case ModalMode.RecallSetup:
        return "Recall Setup";
      case ModalMode.EditLayout:
      default:
        return "Edit Layout";
    }
  }, [modalConfig.mode]);

  return (
    <>
      <ModalHeader className="px-10 pt-10 pb-0">
        <div className="flex w-full justify-between">
          <span className="text-yellow-fp text-lg font-bold uppercase">
            {modalTitle}
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
          {modalConfig.mode === ModalMode.AddApp && newApp ? (
            <NewAppDetails app={newApp} />
          ) : null}
          {modalConfig.mode === ModalMode.RecallSetup &&
          modalConfig.recallDescription ? (
            <div className="mb-12 whitespace-pre-line text-white">
              {modalConfig.recallDescription}
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
                      newAppId={newAppId !== null ? newAppId : undefined}
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
                  newAppId={newAppId !== null ? newAppId : undefined}
                  isDragging={true}
                  setDeletePopoverId={setDeletePopoverId}
                  item={activeItem}
                />
              ) : null}
            </DragOverlay>
          </DndContext>
          <div className="mt-18 flex justify-center">
            {modalConfig.mode === ModalMode.RecallSetup ? (
              <div className="flex gap-4">
                <Switch
                  color="secondary"
                  defaultSelected={recallParams}
                  onChange={(ev) => setRecallParams(ev.target.checked)}
                >
                  Recall all app parameters
                </Switch>
                {modalConfig.recallConfig ? (
                  <Switch
                    color="secondary"
                    defaultSelected={recallConfig}
                    onChange={(ev) => setRecallConfig(ev.target.checked)}
                  >
                    Recall global configuration
                  </Switch>
                ) : null}
              </div>
            ) : (
              <ButtonSecondary className="text-red" onPress={handleClearAll}>
                <Icon name="trash" /> Clear All Apps
              </ButtonSecondary>
            )}
          </div>
        </div>
      </ModalBody>
      <ModalFooter className="flex justify-between px-10">
        {error && <span className="text-danger">{error}</span>}
        <span className="ml-auto">
          <ButtonPrimary
            isLoading={isSubmitting}
            isDisabled={!!error}
            onPress={async () => {
              await handleSave();
              onClose();
            }}
          >
            {modalConfig.mode === ModalMode.RecallSetup ? "Load" : "Save"}
          </ButtonPrimary>
          <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
        </span>
      </ModalFooter>
    </>
  );
};
