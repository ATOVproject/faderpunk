import { useState, type ReactNode } from "react";

import { ModalConfig, ModalMode } from "../utils/types";
import { ModalContext } from "./ModalContext";

interface ModalProviderProps {
  children: ReactNode;
}

export const ModalProvider = ({ children }: ModalProviderProps) => {
  const [modalConfig, setModalConfig] = useState<ModalConfig>({
    isOpen: false,
    mode: ModalMode.EditLayout,
  });

  return (
    <ModalContext.Provider value={{ modalConfig, setModalConfig }}>
      {children}
    </ModalContext.Provider>
  );
};
