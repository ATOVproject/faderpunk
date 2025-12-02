import type {
  Layout,
  GlobalConfig,
  Value,
  FixedLengthArray,
  ConfigMsgOut,
} from "@atov/fp-config";

import type {
  AllApps,
  App,
  AppLayout,
  AppParams,
  LayoutFile,
  ParamValues,
  RecoveredLayout,
} from "../utils/types";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "../utils/usb-protocol";
import { getFixedLengthParamArray } from "./utils";
import { parseParamValueFromFile } from "./validators";

const LAYOUT_VERSION = 1;

export const setGlobalConfig = async (dev: USBDevice, config: GlobalConfig) => {
  await sendMessage(dev, {
    tag: "SetGlobalConfig",
    value: config,
  });
};

export const getAllApps = async (dev: USBDevice) => {
  const response = await sendAndReceive(dev, {
    tag: "GetAllApps",
  });

  if (response.tag !== "BatchMsgStart") {
    throw new Error(
      `Could not fetch apps. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  const apps = await receiveBatchMessages(dev, response.value);

  // Parse apps data into a Map for easy lookup by app ID
  const parsedApps = new Map<number, App>();

  apps
    .filter(
      (item): item is Extract<typeof item, { tag: "AppConfig" }> =>
        item.tag === "AppConfig",
    )
    .forEach((app) => {
      const appConfig = {
        appId: app.value[0],
        channels: app.value[1],
        paramCount: app.value[2][0],
        name: app.value[2][1] as string,
        description: app.value[2][2] as string,
        color: app.value[2][3].tag,
        icon: app.value[2][4].tag,
        params: app.value[2][5],
      };

      parsedApps.set(appConfig.appId, appConfig);
    });

  return parsedApps;
};

export const getAppParams = async (dev: USBDevice, layoutId: number) => {
  const response = await sendAndReceive(dev, {
    tag: "GetAppParams",
    value: { layout_id: layoutId },
  });

  if (response.tag !== "AppState") {
    throw new Error(
      `Could not fetch app params. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  return response.value[1];
};

export const setAppParams = async (
  dev: USBDevice,
  layoutId: number,
  values: FixedLengthArray<Value | undefined, 8>,
) => {
  const response = await sendAndReceive(dev, {
    tag: "SetAppParams",
    value: {
      layout_id: layoutId,
      values,
    },
  });

  if (response.tag !== "AppState") {
    throw new Error(
      `Could not fetch app params. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  return response.value[1];
};

export const getAllAppParams = async (
  dev: USBDevice,
): Promise<Map<number, Value[]>> => {
  const response = await sendAndReceive(dev, {
    tag: "GetAllAppParams",
  });

  if (response.tag !== "BatchMsgStart") {
    throw new Error(
      `Could not fetch apps. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  const messages = await receiveBatchMessages(dev, response.value);
  const params = messages
    .filter((item): item is AppParams => item.tag === "AppState")
    .map(({ value }) => value);

  return new Map(params);
};

export const getGlobalConfig = async (dev: USBDevice) => {
  const response = await sendAndReceive(dev, {
    tag: "GetGlobalConfig",
  });

  if (response.tag !== "GlobalConfig") {
    throw new Error(
      `Could not fetch app params. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  return response.value;
};

export const setAllAppParams = async (dev: USBDevice, params: ParamValues) => {
  const allParams: [number, Value[]][] = Array.from(params.entries());
  for (let i = 0; i < allParams.length; i++) {
    const [layoutId, values] = allParams[i];
    const fixedLengthValues = getFixedLengthParamArray(values);
    await setAppParams(dev, layoutId, fixedLengthValues);
  }
};

const transformLayout = (
  response: Extract<ConfigMsgOut, { tag: "Layout" }>,
  allApps: AllApps,
) => {
  const layout: AppLayout = [];
  let lastUsed = -1;
  let nextEmptyId = 16;

  response.value[0].forEach((slot, idx) => {
    if (idx <= lastUsed) {
      return;
    }
    if (!slot) {
      lastUsed++;
      layout.push({
        id: nextEmptyId++,
        app: null,
        startChannel: idx,
      });
      return;
    }
    const [appId, channels, layoutId] = slot;
    const app = allApps.get(appId);
    if (!app) {
      lastUsed++;
      layout.push({
        id: nextEmptyId++,
        app: null,
        startChannel: idx,
      });
      return;
    }
    lastUsed = idx + Number(channels) - 1;

    layout.push({ id: layoutId, app, startChannel: idx });
  });

  return layout;
};

export const getLayout = async (
  dev: USBDevice,
  allApps: AllApps,
): Promise<AppLayout> => {
  const response = await sendAndReceive(dev, {
    tag: "GetLayout",
  });

  if (response.tag !== "Layout") {
    throw new Error(
      `Could not fetch layout. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  return transformLayout(response, allApps);
};

export const setLayout = async (
  dev: USBDevice,
  layout: AppLayout,
  allApps: AllApps,
) => {
  const sendLayout: Layout = [
    [
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    ],
  ];

  let currentChan = 0;
  layout.forEach((appSlot) => {
    if (currentChan >= 16) {
      // Safeguard if for some reason the layout is messed up
      return;
    }
    if (appSlot.app) {
      sendLayout[0][currentChan] = [
        appSlot.app.appId,
        appSlot.app.channels,
        appSlot.id,
      ];
      currentChan += Number(appSlot.app.channels);
    } else {
      currentChan++;
    }
  });

  const response = await sendAndReceive(dev, {
    tag: "SetLayout",
    value: sendLayout,
  });

  if (response.tag !== "Layout") {
    throw new Error(
      `Could not fetch layout. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  return transformLayout(response, allApps);
};

export const saveLayout = (
  layout: AppLayout,
  params: ParamValues,
  config?: GlobalConfig,
  description?: string,
): LayoutFile => {
  const layoutFile: LayoutFile = {
    version: LAYOUT_VERSION,
    layout: [],
    config,
    description,
  };

  layout.forEach(({ id, app, startChannel }) => {
    const appParams = params.get(id);
    layoutFile.layout.push({
      layoutId: id,
      appId: !appParams || !app?.appId ? null : app.appId,
      startChannel,
      params: params.get(id) || null,
    });
  });

  return layoutFile;
};

export const recoverLayout = (
  layoutFile: LayoutFile,
  allApps: AllApps,
): RecoveredLayout => {
  const layout: AppLayout = [];
  const allParams: ParamValues = new Map();
  layoutFile.layout.forEach(({ appId, layoutId, params, startChannel }) => {
    if (!appId) {
      layout.push({
        id: layoutId,
        app: null,
        startChannel,
      });
    } else {
      const app = allApps.get(appId);
      if (!app || !params) {
        layout.push({
          id: layoutId,
          app: null,
          startChannel,
        });
      } else {
        const appParams = app.params.map((param, idx) => {
          return parseParamValueFromFile(param, params[idx]);
        });
        layout.push({ id: layoutId, app, startChannel });

        allParams.set(layoutId, appParams);
      }
    }
  });
  return {
    layout,
    params: allParams,
    config: layoutFile.config,
    description: layoutFile.description,
  };
};

// Serialize with BigInt support
export const serializeLayout = (layoutFile: LayoutFile) => {
  return JSON.stringify(
    layoutFile,
    (_key, value) => {
      if (typeof value === "bigint") {
        return { __bigint: value.toString() };
      }
      return value;
    },
    2,
  );
};

// Deserialize with BigInt support
export const deserializeLayout = (json: string) => {
  return JSON.parse(json, (_key, value) => {
    if (value && typeof value === "object" && "__bigint" in value) {
      return BigInt(value.__bigint);
    }
    return value;
  });
};

export const factoryReset = async (dev: USBDevice) => {
  await sendMessage(dev, {
    tag: "FactoryReset",
  });
};
