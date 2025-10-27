import { useCallback, useEffect, useState } from "react";
import { Modal, ModalContent } from "@heroui/modal";
import { Tabs, Tab } from "@heroui/tabs";
import { addToast, closeAll } from "@heroui/toast";
import semverLt from "semver/functions/lt";
import { useNavigate } from "react-router-dom";

import { FIRMWARE_MIN_SUPPORTED, FIRMWARE_LATEST_VERSION } from "../consts";
import { useStore } from "../store";
import { getDeviceVersion } from "../utils/usb-protocol";
import { Layout } from "./Layout";
import { DeviceTab } from "./DeviceTab";
import { AppsTab } from "./AppsTab";
import { SettingsTab } from "./SettingsTab";
import { EditLayoutModal } from "./EditLayoutModal";
import { ManualTab } from "./ManualTab";
import { UpdateGuide } from "./manual/UpdateGuide";

export const ConfiguratorPage = () => {
  const { apps, config, setLayout, layout, usbDevice } = useStore();
  const [modalApp, setModalApp] = useState<number | null>(null);
  const navigate = useNavigate();

  const handleModalOpen = useCallback(
    (isOpen: boolean) => setModalApp(isOpen ? -1 : null),
    [],
  );

  const handleToastClick = useCallback(() => {
    closeAll();
    navigate("/update");
  }, [navigate]);

  const version = usbDevice && getDeviceVersion(usbDevice);
  const updateRequired =
    version && semverLt(version, FIRMWARE_MIN_SUPPORTED);
  const updatedAvailable =
    version && semverLt(version, FIRMWARE_LATEST_VERSION);

  useEffect(() => {
    if (!usbDevice) {
      navigate("/");
      return;
    }

    if (updateRequired) {
      navigate("/update");
      return;
    }

    if (updatedAvailable) {
      addToast({
        title: (
          <span className="cursor-pointer" onClick={handleToastClick}>
            New firmware available
          </span>
        ),
        description: (
          <span className="cursor-pointer" onClick={handleToastClick}>
            Firmware version {FIRMWARE_LATEST_VERSION} is available. Click here
            to update.
          </span>
        ),
        timeout: Infinity,
        color: "danger",
      });
    }

    return () => {
      closeAll();
    };
  }, [navigate, handleToastClick, usbDevice, updatedAvailable, updateRequired]);

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
          {updatedAvailable ? (
            <Tab key="update" title="Update">
              <UpdateGuide />
            </Tab>
          ) : null}
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
