import { create } from "zustand";

import type { AllApps, AppLayout } from "./utils/types";
import { connectToFaderPunk } from "./utils/usb-protocol";
import { getAllApps, getLayout } from "./utils/config";

interface AppState {
  usbDevice: USBDevice | undefined;
  apps: AllApps | undefined;
  layout: AppLayout | undefined;
  connect: () => Promise<void>;
  setLayout: (layout: AppLayout) => void;
}

const initialState = {
  usbDevice: undefined,
  apps: undefined,
  layout: undefined,
};

export const useStore = create<AppState>((set) => ({
  ...initialState,
  connect: async () => {
    try {
      const device = await connectToFaderPunk();
      const allApps = await getAllApps(device);
      const appLayout = await getLayout(device, allApps);
      set({ usbDevice: device, apps: allApps, layout: appLayout });
    } catch (error) {
      console.error("Failed to connect to device:", error);
      // Reset state on failure
      set({ usbDevice: undefined, apps: undefined, layout: undefined });
    }
  },
  setLayout: (layout) => console.log(layout) || set({ layout }),
}));
