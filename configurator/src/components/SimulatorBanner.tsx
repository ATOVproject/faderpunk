import {
  Modal,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
} from "@heroui/modal";

import { useConnectDevice } from "../utils/useConnectDevice";
import { ButtonPrimary, ButtonSecondary } from "./Button";

export const SimulatorBanner = () => {
  const {
    connect,
    connecting,
    webUsbSupported,
    updateAvailable,
    dismissUpdate,
    updateFirmware,
    continueAnyway,
  } = useConnectDevice();

  return (
    <>
      <div className="bg-yellow-fp mb-4 flex flex-col items-center justify-center gap-2 rounded-sm px-4 py-3 text-center text-sm font-bold text-black sm:flex-row">
        <span>
          Simulator mode — changes aren't sent to any hardware.{" "}
          {webUsbSupported
            ? "Connect your Faderpunk to configure the real thing."
            : "Use a Chromium browser (Chrome or Edge) to connect a device."}
        </span>
        <button
          onClick={connect}
          disabled={connecting || !webUsbSupported}
          className="shrink-0 cursor-pointer rounded-sm bg-black px-4 py-1.5 font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {connecting ? "Connecting…" : "Connect Device"}
        </button>
      </div>

      <Modal
        isOpen={!!updateAvailable}
        onClose={dismissUpdate}
        backdrop="blur"
        radius="sm"
        hideCloseButton
      >
        <ModalContent>
          <ModalHeader className="text-white">
            Firmware update available
          </ModalHeader>
          <ModalBody className="text-default-300 text-sm">
            {updateAvailable && (
              <p>
                Your device is running firmware{" "}
                <span className="text-pink-fp font-semibold">
                  v{updateAvailable.currentVersion}
                </span>
                . Version{" "}
                <span className="text-pink-fp font-semibold">
                  v{updateAvailable.latestVersion}
                </span>{" "}
                is available. Update for the latest features, or continue with
                the matching configurator.
              </p>
            )}
          </ModalBody>
          <ModalFooter>
            <ButtonSecondary onPress={continueAnyway}>
              Continue anyway
            </ButtonSecondary>
            <ButtonPrimary onPress={updateFirmware}>
              Update firmware
            </ButtonPrimary>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </>
  );
};
