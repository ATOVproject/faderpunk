import type { PropsWithChildren } from "react";
import { Modal, ModalContent } from "@heroui/modal";

import { useStore } from "../store";
import { EditLayoutModal } from "../components/app-layout/EditLayoutModal";

interface Props {
  isModalOpen: boolean;
  onModalOpenChange(): void;
}

export const Layout = ({
  children,
  isModalOpen,
  onModalOpenChange,
}: PropsWithChildren<Props>) => {
  const { setLayout, layout } = useStore();
  return (
    <main className="min-h-screen bg-gray-500 text-white">
      <div className="mx-auto max-w-6xl py-14">{children}</div>
      {layout ? (
        <Modal
          // size="5xl"
          className="max-w-6xl"
          isOpen={isModalOpen}
          backdrop="blur"
          onOpenChange={onModalOpenChange}
          hideCloseButton
          radius="sm"
        >
          <ModalContent>
            {(onClose) => (
              <EditLayoutModal
                onSave={setLayout}
                initialLayout={layout}
                onClose={onClose}
              />
            )}
          </ModalContent>
        </Modal>
      ) : null}
    </main>
  );
};
