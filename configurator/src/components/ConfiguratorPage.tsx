import { useCallback, useState } from "react";
import { Tabs, Tab } from "@heroui/tabs";

import { Layout } from "./Layout";
import { useStore } from "../store";
import { DeviceTab } from "./DeviceTab";
import { AppsTab } from "./AppsTab";
import { SettingsTab } from "./SettingsTab";
import { Modal, ModalContent } from "@heroui/modal";
import { EditLayoutModal } from "./EditLayoutModal";
import { ManualTab } from "./ManualTab";

export const ConfiguratorPage = () => {
  const { apps, config, setLayout, layout } = useStore();
  const [modalApp, setModalApp] = useState<number | null>(null);

  const handleModalOpen = useCallback(
    (isOpen: boolean) => setModalApp(isOpen ? -1 : null),
    [],
  );

  return (
    <Layout>
      <>
        <Tabs
          className="border-default-100 mb-8 w-full border-b-3"
          classNames={{
            tabList: "flex p-0 rounded-none gap-0",
            cursor: "rounded-none rounded-t-md dark:bg-black",
            tab: "px-12 py-6",
            tabContent: "text-white font-bold uppercase text-lg",
          }}
          variant="light"
        >
          <Tab key="device" title="Device">
            <DeviceTab layout={layout} setModalApp={setModalApp} />
          </Tab>
          <Tab key="apps" title="Apps">
            <AppsTab apps={apps} layout={layout} setModalApp={setModalApp} />
          </Tab>
          <Tab key="settings" title="Settings">
            <SettingsTab config={config} />
          </Tab>
          <Tab key="manual" title="Manual">
            <ManualTab />
          </Tab>
        </Tabs>
        {layout ? (
          <Modal
            // size="5xl"
            className="max-w-6xl"
            isOpen={!!modalApp}
            backdrop="blur"
            onOpenChange={handleModalOpen}
            hideCloseButton
            radius="sm"
          >
            <ModalContent>
              {(onClose) => (
                <EditLayoutModal
                  onSave={setLayout}
                  initialLayout={layout}
                  onClose={onClose}
                  modalApp={modalApp}
                />
              )}
            </ModalContent>
          </Modal>
        ) : null}
      </>
    </Layout>
  );
};
