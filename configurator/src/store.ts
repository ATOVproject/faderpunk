import { create } from "zustand";
import { Value, type GlobalConfig } from "@atov/fp-config";

import type { AllApps, AppLayout, ParamValues } from "./utils/types";
import { connectToFaderPunk, getDeviceVersion } from "./utils/usb-protocol";
import {
  getAllAppParams,
  getAllApps,
  getGlobalConfig,
  getLayout,
} from "./utils/config";

interface State {
  apps: AllApps | undefined;
  connect: () => Promise<void>;
  config: GlobalConfig | undefined;
  deviceVersion: string | undefined;
  layout: AppLayout | undefined;
  params: ParamValues | undefined;
  setLayout: (layout: AppLayout) => void;
  setParams: (id: number, newParams: Value[]) => void;
  setAllParams: (newParams: ParamValues) => void;
  usbDevice: USBDevice | undefined;
}

const initialState = {
  apps: undefined,
  config: undefined,
  deviceVersion: undefined,
  layout: undefined,
  params: undefined,
  usbDevice: undefined,
};

export const useStore = create<State>((set) => ({
  ...initialState,
  connect: async () => {
    try {
      const device = await connectToFaderPunk();
      const deviceVersion = getDeviceVersion(device);
      const apps = await getAllApps(device);
      const params = await getAllAppParams(device);
      const layout = await getLayout(device, apps);
      const config = await getGlobalConfig(device);
      set({ apps, config, deviceVersion, layout, params, usbDevice: device });
    } catch (error) {
      console.error("Failed to connect to device:", error);
      // Reset state on failure
      set({
        apps: undefined,
        config: undefined,
        deviceVersion: undefined,
        layout: undefined,
        params: undefined,
        usbDevice: undefined,
      });
    }
  },
  setLayout: (layout) => set({ layout }),
  setParams: (id, newParams) =>
    set(({ params }) => ({ params: new Map(params).set(id, newParams) })),
  setAllParams: (newParams) => set({ params: newParams }),
}));
