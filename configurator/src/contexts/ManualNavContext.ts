import { createContext, useContext } from "react";

interface ManualNav {
  // Switch to the Manual tab and scroll to the element with this id.
  showInManual: (id: string) => void;
}

export const ManualNavContext = createContext<ManualNav>({
  showInManual: () => {},
});

export const useManualNav = () => useContext(ManualNavContext);
