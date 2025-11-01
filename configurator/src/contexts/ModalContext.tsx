import { createContext, useContext } from "react";
import { ModalConfig } from "../utils/types";

interface ModalContextType {
  modalConfig: ModalConfig;
  setModalConfig: (config: ModalConfig) => void;
}

export const ModalContext = createContext<ModalContextType | undefined>(
  undefined,
);

export const useModalContext = () => {
  const context = useContext(ModalContext);
  if (!context) {
    throw new Error("useModalContext must be used within a ModalProvider");
  }
  return context;
};
