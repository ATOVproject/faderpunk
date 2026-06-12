import { useCallback, useEffect } from "react";
import { Modal, ModalContent } from "@heroui/modal";
import { Tabs, Tab } from "@heroui/tabs";
import { useNavigate } from "react-router-dom";

import { useModalContext } from "../contexts/ModalContext";
import { ModalProvider } from "../contexts/ModalProvider";
import { useStore } from "../store";
import { IS_SIMULATOR_BUILD } from "../consts";
import { ModalMode } from "../utils/types";
import { Layout } from "./Layout";
import { DeviceTab } from "./DeviceTab";
import { AppsTab } from "./AppsTab";
import { SettingsTab } from "./SettingsTab";
import { EditLayoutModal } from "./EditLayoutModal";
import { ManualTab } from "./ManualTab";

const ConfiguratorPageContent = () => {
  const { apps, config, setLayout, layout, usbDevice, isSimulator } =
    useStore();
  const { modalConfig, setModalConfig } = useModalContext();
  const navigate = useNavigate();

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
    if (!usbDevice && !isSimulator) {
      navigate("/");
    }
  }, [navigate, usbDevice, isSimulator]);

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
        </Tabs>
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
