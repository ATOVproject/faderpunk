import { create } from "zustand";
import { Value, type GlobalConfig } from "@atov/fp-config";

import type { AllApps, AppLayout, AppSlot, ParamValues } from "./utils/types";
import {
  connectToFaderPunk,
  getDeviceVersion,
  tryAutoConnect,
  type FpMidiDevice,
} from "./utils/midi-protocol";
import {
  getAllAppParams,
  getAllApps,
  getGlobalConfig,
  getLayout,
  saveLayout,
  recoverLayout,
  serializeLayout,
  deserializeLayout,
} from "./utils/config";
import { DEMO_APPS } from "./demo/catalog";
import { defaultGlobalConfig } from "./utils/validators";
import { IS_SIMULATOR_BUILD } from "./consts";

const makeEmptyLayout = (): AppLayout =>
  Array.from(
    { length: 16 },
    (_, i): AppSlot => ({
      id: i,
      app: null,
      startChannel: i,
    }),
  );

// All deployments (/1.9, /beta, /simulator, …) share one gh-pages origin and
// thus one localStorage, so the key is namespaced by deploy path.
const SIMULATOR_STORAGE_KEY = `fp-simulator-state:${import.meta.env.BASE_URL}`;

const persistSimulatorState = (
  layout: AppLayout,
  params: ParamValues,
  config: GlobalConfig,
) => {
  try {
    localStorage.setItem(
      SIMULATOR_STORAGE_KEY,
      serializeLayout(saveLayout(layout, params, config)),
    );
  } catch {
    // ignore storage quota errors
  }
};

const loadPersistedSimulatorState = () => {
  try {
    const raw = localStorage.getItem(SIMULATOR_STORAGE_KEY);
    if (!raw) return null;
    return recoverLayout(deserializeLayout(raw), DEMO_APPS);
  } catch {
    return null;
  }
};

interface State {
  apps: AllApps | undefined;
  autoConnect: () => Promise<boolean>;
  connect: () => Promise<void>;
  connectSimulator: () => void;
  config: GlobalConfig | undefined;
  disconnect: () => void;
  deviceVersion: string | undefined;
  isSimulator: boolean;
  layout: AppLayout | undefined;
  params: ParamValues | undefined;
  setConfig: (config: GlobalConfig) => void;
  setLayout: (layout: AppLayout) => void;
  setParams: (id: number, newParams: Value[]) => void;
  setAllParams: (newParams: ParamValues) => void;
  device: FpMidiDevice | undefined;
}

const initialState = {
  apps: undefined,
  config: undefined,
  deviceVersion: undefined,
  isSimulator: false,
  layout: undefined,
  params: undefined,
  device: undefined,
};

export const useStore = create<State>((set, get) => ({
  ...initialState,
  autoConnect: async () => {
    try {
      const device = await tryAutoConnect();
      if (!device) return false;

      const deviceVersion = getDeviceVersion(device);
      set({ deviceVersion });

      const apps = await getAllApps(device);
      const params = await getAllAppParams(device);
      const layout = await getLayout(device, apps);
      const config = await getGlobalConfig(device);

      set({ apps, config, deviceVersion, layout, params, device });
      return true;
    } catch (error) {
      console.error("Auto-connect failed:", error);
      return false;
    }
  },
  connect: async () => {
    try {
      const device = await connectToFaderPunk();
      const deviceVersion = getDeviceVersion(device);

      set({ deviceVersion });

      const apps = await getAllApps(device);
      const params = await getAllAppParams(device);
      const layout = await getLayout(device, apps);
      const config = await getGlobalConfig(device);
      set({ apps, config, deviceVersion, layout, params, device });
    } catch (error) {
      console.error("Failed to connect to device:", error);
      // Reset state on failure
      set({
        apps: undefined,
        config: undefined,
        deviceVersion: undefined,
        layout: undefined,
        params: undefined,
        device: undefined,
      });
    }
  },
  connectSimulator: () => {
    const saved = loadPersistedSimulatorState();
    set({
      isSimulator: true,
      apps: DEMO_APPS,
      deviceVersion: "simulator",
      device: undefined,
      layout: saved?.layout ?? makeEmptyLayout(),
      params: saved?.params ?? new Map(),
      config: saved?.config ?? defaultGlobalConfig,
    });
  },
  disconnect: () => {
    // Reset in-memory state only. Persisted simulator work is intentionally
    // kept so "Open Simulator" can resume it; a real-device disconnect must
    // not wipe it either.
    set({ ...initialState });
    // The dedicated simulator build has no connect page to return to, so
    // drop straight back into a simulator session.
    if (IS_SIMULATOR_BUILD) get().connectSimulator();
  },
  setConfig: (config) => {
    set({ config });
    const { isSimulator, layout, params } = get();
    if (isSimulator && layout && params)
      persistSimulatorState(layout, params, config);
  },
  setLayout: (layout) => {
    set({ layout });
    const { isSimulator, params, config } = get();
    if (isSimulator && params && config)
      persistSimulatorState(layout, params, config);
  },
  setParams: (id, newParams) => {
    const { params, isSimulator, layout, config } = get();
    const updated = new Map(params).set(id, newParams);
    set({ params: updated });
    if (isSimulator && layout && config)
      persistSimulatorState(layout, updated, config);
  },
  setAllParams: (newParams) => {
    set({ params: newParams });
    const { isSimulator, layout, config } = get();
    if (isSimulator && layout && config)
      persistSimulatorState(layout, newParams, config);
  },
}));
