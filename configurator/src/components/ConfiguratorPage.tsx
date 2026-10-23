import { useCallback, useEffect, useMemo, useState } from "react";
import { Modal, ModalContent } from "@heroui/modal";
import { Tabs, Tab } from "@heroui/tabs";
import { useLocation, useNavigate } from "react-router-dom";

import { useModalContext } from "../contexts/ModalContext";
import { ManualNavContext } from "../contexts/ManualNavContext";
import { ModalProvider } from "../contexts/ModalProvider";
import { useStore } from "../store";
import { IS_SIMULATOR_BUILD } from "../consts";
import { ModalMode } from "../utils/types";
import { Layout } from "./Layout";
import { DeviceTab } from "./DeviceTab";
import { AppsTab } from "./AppsTab";
import { CommunityTab } from "./CommunityTab";
import { SettingsTab } from "./SettingsTab";
import { EditLayoutModal } from "./EditLayoutModal";
import { ManualTab } from "./ManualTab";

const ConfiguratorPageContent = () => {
  const { apps, config, setLayout, layout, device, isSimulator } = useStore();
  const { modalConfig, setModalConfig } = useModalContext();
  const navigate = useNavigate();
  const location = useLocation();
  const [selectedTab, setSelectedTab] = useState("device");

  // ManualTab scrolls to whatever the URL hash names when it mounts, so
  // setting the hash and then switching to it is all that's needed.
  const manualNav = useMemo(
    () => ({
      showInManual: (id: string) => {
        navigate(`${location.pathname}${location.search}#${id}`);
        setSelectedTab("manual");
      },
    }),
    [navigate, location.pathname, location.search],
  );

  const handleModalOpen = useCallback(
    (isOpen: boolean) => {
      if (isOpen) {
        setModalConfig({ ...modalConfig, isOpen });
      } else {
        setModalConfig({ isOpen, mode: ModalMode.EditLayout });
      }
    },
    [modalConfig, setModalConfig],
  );

  useEffect(() => {
    if (!device && !isSimulator) {
      navigate("/");
    }
  }, [navigate, device, isSimulator]);

  const initialLayout = modalConfig.recallLayout || layout;

  return (
    <Layout>
      <>
        {isSimulator && (
          <div className="bg-yellow-fp mb-4 rounded-sm px-4 py-2 text-center text-sm font-bold text-black">
            Simulator — no device connected. Changes are not sent to hardware.
            {IS_SIMULATOR_BUILD && (
              <>
                {" "}
                Have a device?{" "}
                <a href="/" className="underline">
                  Open the configurator
                </a>
                .
              </>
            )}
          </div>
        )}
        <ManualNavContext.Provider value={manualNav}>
          <Tabs
            selectedKey={selectedTab}
            onSelectionChange={(key) => setSelectedTab(String(key))}
            className="border-default-100 sticky top-0 z-30 mb-8 flex w-full border-b-3 bg-gray-500"
            classNames={{
              tabList: "flex p-0 rounded-none gap-0",
              cursor: "rounded-none rounded-t-md dark:bg-black",
              tab: "px-12 py-6",
              tabContent: "text-white font-bold uppercase text-lg",
            }}
            variant="light"
          >
            <Tab key="device" title="Device">
              <DeviceTab layout={layout} />
            </Tab>
            <Tab key="apps" title="Apps">
              <AppsTab apps={apps} layout={layout} />
            </Tab>
            <Tab key="settings" title="Settings">
              <SettingsTab config={config} />
            </Tab>
            <Tab key="manual" title="Manual">
              <ManualTab />
            </Tab>
            <Tab key="community" title="Community">
              <CommunityTab apps={apps} />
            </Tab>
          </Tabs>
        </ManualNavContext.Provider>
        {initialLayout ? (
          <Modal
            className="max-w-6xl"
            isOpen={modalConfig.isOpen}
            backdrop="blur"
            onOpenChange={handleModalOpen}
            hideCloseButton
            radius="sm"
          >
            <ModalContent>
              {(onClose) => (
                <EditLayoutModal
                  onSave={setLayout}
                  initialLayout={initialLayout}
                  onClose={onClose}
                  modalConfig={modalConfig}
                />
              )}
            </ModalContent>
          </Modal>
        ) : null}
      </>
    </Layout>
  );
};

export const ConfiguratorPage = () => {
  return (
    <ModalProvider>
      <ConfiguratorPageContent />
    </ModalProvider>
  );
};
