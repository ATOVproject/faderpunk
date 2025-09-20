import { create } from "zustand";
import { type GlobalConfig } from "@atov/fp-config";

import type { AllApps, AppLayout } from "./utils/types";
import { connectToFaderPunk } from "./utils/usb-protocol";
import { getAllApps, getGlobalConfig, getLayout } from "./utils/config";

interface AppState {
  apps: AllApps | undefined;
  connect: () => Promise<void>;
  config: GlobalConfig | undefined;
  layout: AppLayout | undefined;
  setLayout: (layout: AppLayout) => void;
  usbDevice: USBDevice | undefined;
}

const initialState = {
  apps: undefined,
  config: undefined,
  layout: undefined,
  usbDevice: undefined,
};

export const useStore = create<AppState>((set) => ({
  ...initialState,
  connect: async () => {
    try {
      const device = await connectToFaderPunk();
      const apps = await getAllApps(device);
      const layout = await getLayout(device, apps);
      const config = await getGlobalConfig(device);
      set({ apps, config, layout, usbDevice: device });
    } catch (error) {
      console.error("Failed to connect to device:", error);
      // Reset state on failure
      set({
        apps: undefined,
        config: undefined,
        layout: undefined,
        usbDevice: undefined,
      });
    }
  },
  setLayout: (layout) => set({ layout }),
}));
