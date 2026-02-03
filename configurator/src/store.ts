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
import { NavigateFunction } from "react-router-dom";

interface State {
  apps: AllApps | undefined;
  connect: (navigate: NavigateFunction) => Promise<void>;
  config: GlobalConfig | undefined;
  disconnect: () => void;
  deviceVersion: string | undefined;
  layout: AppLayout | undefined;
  params: ParamValues | undefined;
  setLayout: (layout: AppLayout) => void;
  setConfig: (config: GlobalConfig) => void;
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
  connect: async (_navigate: NavigateFunction) => {
    try {
      const device = await connectToFaderPunk();
      const deviceVersion = getDeviceVersion(device);

      set({ deviceVersion });

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
  disconnect: () => {
    set({
      apps: undefined,
      config: undefined,
      deviceVersion: undefined,
      layout: undefined,
      params: undefined,
      usbDevice: undefined,
    });
  },
  setLayout: (layout) => set({ layout }),
  setConfig: (config) => set({ config }),
  setParams: (id, newParams) =>
    set(({ params }) => ({ params: new Map(params).set(id, newParams) })),
  setAllParams: (newParams) => set({ params: newParams }),
}));
