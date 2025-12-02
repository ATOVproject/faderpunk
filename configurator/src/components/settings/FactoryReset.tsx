import { type ChangeEvent, useCallback, useState } from "react";

import { ButtonPrimary, ButtonSecondary } from "../Button";
import { factoryReset } from "../../utils/config";
import { useStore } from "../../store";
import {
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/modal";
import { Switch } from "@heroui/switch";
import { delay } from "../../utils/utils";
import { useNavigate } from "react-router-dom";

export const FactoryReset = () => {
  const { disconnect, usbDevice } = useStore();
  const navigate = useNavigate();
  const [isOpen, setOpen] = useState(false);
  const [isSure, setSure] = useState(false);
  const [isLoading, setLoading] = useState(false);

  const handleConfirm = useCallback(async () => {
    if (!usbDevice) {
      return;
    }
    setLoading(true);
    try {
      await factoryReset(usbDevice);
      // 3 seconds should be plenty enough
      await delay(5000);
      setSure(false);
      setOpen(false);
      disconnect();
      navigate("/");
    } catch (error) {
      console.error(error);
    } finally {
      setLoading(false);
    }
  }, [disconnect, navigate, usbDevice]);

  const handleOpenChange = useCallback((shouldOpen: boolean) => {
    setSure(false);
    setOpen(shouldOpen);
  }, []);

  const handleButtonPress = useCallback(() => {
    setOpen(true);
  }, []);

  const handleSureChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setSure(e.target.checked);
  }, []);

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Factory Reset
      </h2>
      <div className="mb-12 px-4">
        <ButtonPrimary onPress={handleButtonPress}>
          Reset to factory settings
        </ButtonPrimary>
      </div>
      <Modal isOpen={isOpen} onOpenChange={handleOpenChange}>
        <ModalContent>
          {(onClose) => (
            <>
              <ModalHeader>Factory reset</ModalHeader>
              <ModalBody>
                Are you sure you want to perform a full factory reset? All
                settings will be deleted. This action can't be undone.
                <Switch
                  defaultSelected={isSure}
                  color="danger"
                  onChange={handleSureChange}
                >
                  I'm sure
                </Switch>
                <span className="text-sm">
                  The device will restart after the factory reset.
                </span>
              </ModalBody>
              <ModalFooter>
                <ButtonPrimary
                  isLoading={isLoading}
                  isDisabled={!isSure}
                  color="danger"
                  onPress={handleConfirm}
                >
                  Delete
                </ButtonPrimary>
                <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </div>
  );
};
