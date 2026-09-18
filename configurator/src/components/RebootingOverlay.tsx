import { useEffect } from "react";
import { Modal, ModalBody, ModalContent, ModalHeader } from "@heroui/modal";

import { useStore } from "../store";

// Last-resort backstop, not part of normal operation: every caller sets
// `rebooting` via store.ts's resyncAfterReboot, which always clears it
// itself once its own retry loop resolves (success or exhausted budget) —
// this timer only matters if that never runs to completion at all (e.g. an
// exception thrown before it's reached). Deliberately well above
// resyncAfterReboot's own retry budget so it can't fire first and close
// the overlay out from under a caller still legitimately retrying.
const REBOOT_GRACE_MS = 30000;

// Shown for any firmware-initiated sys_reset() the client knows about in
// advance — a SetLayout reply with `rebooting: true` (see #675's
// arena-budget check), or a factory reset. Without this, the resulting
// brief, deliberate disconnect looks identical to a real dropped
// connection.
export const RebootingOverlay = () => {
  const { rebooting, setRebooting } = useStore();

  useEffect(() => {
    if (!rebooting) return;
    const timeout = setTimeout(() => setRebooting(false), REBOOT_GRACE_MS);
    return () => clearTimeout(timeout);
  }, [rebooting, setRebooting]);

  return (
    <Modal
      isOpen={rebooting}
      isDismissable={false}
      isKeyboardDismissDisabled
      hideCloseButton
    >
      <ModalContent>
        <ModalHeader>Unit rebooting</ModalHeader>
        <ModalBody className="pb-6 text-sm text-gray-400">
          The device is restarting. This will only take a moment.
        </ModalBody>
      </ModalContent>
    </Modal>
  );
};
