import { create } from "zustand";
import { type GlobalConfig } from "@atov/fp-config";

import type { AllApps, AppLayout } from "./utils/types";
import { connectToFaderPunk, getDeviceVersion } from "./utils/usb-protocol";
import { getAllApps, getGlobalConfig, getLayout } from "./utils/config";

interface State {
  apps: AllApps | undefined;
  connect: (navigate: (path: string) => void) => Promise<void>;
  config: GlobalConfig | undefined;
  layout: AppLayout | undefined;
  setLayout: (layout: AppLayout) => void;
  usbDevice: USBDevice | undefined;
  deviceVersion: string | undefined;
}

const initialState = {
  apps: undefined,
  config: undefined,
  layout: undefined,
  usbDevice: undefined,
  deviceVersion: undefined,
};

export const useStore = create<State>((set) => ({
  ...initialState,
  connect: async () => {
    try {
      const device = await connectToFaderPunk();
      const deviceVersion = getDeviceVersion(device);
      const apps = await getAllApps(device);
      const layout = await getLayout(device, apps);
      const config = await getGlobalConfig(device);
      set({ apps, config, layout, usbDevice: device, deviceVersion });
    } catch (error) {
      console.error("Failed to connect to device:", error);
      // Reset state on failure
      set({
        apps: undefined,
        config: undefined,
        layout: undefined,
        usbDevice: undefined,
        deviceVersion: undefined,
      });
    }
  },
  setLayout: (layout) => set({ layout }),
}));
