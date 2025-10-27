import type { Layout, GlobalConfig } from "@atov/fp-config";

import type { AllApps, App, AppLayout } from "../utils/types";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "../utils/usb-protocol";
import { transformParamValues } from "./utils";

export const setGlobalConfig = async (dev: USBDevice, config: GlobalConfig) => {
  await sendMessage(dev, {
    tag: "SetGlobalConfig",
    value: config,
  });
};

export const setLayout = async (dev: USBDevice, layout: AppLayout) => {
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

  await sendMessage(dev, {
    tag: "SetLayout",
    value: sendLayout,
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
  paramValues: Record<string, string | boolean>,
) => {
  const values = transformParamValues(paramValues);
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

export const getLayout = async (
  dev: USBDevice,
  apps: AllApps,
): Promise<AppLayout> => {
  const response = await sendAndReceive(dev, {
    tag: "GetLayout",
  });

  if (response.tag !== "Layout") {
    throw new Error(
      `Could not fetch layout. Unexpected repsonse tag: ${response.tag}`,
    );
  }

  const layout: AppLayout = [];
  let lastUsed = -1;
  let nextEmptyId = 16;

  response.value[0].forEach((slot, idx) => {
    if (idx <= lastUsed) {
      return;
    }
    if (!slot) {
      lastUsed++;
      layout.push({ id: nextEmptyId++, app: null, startChannel: idx });
      return;
    }
    const [appId, channels, layoutId] = slot;
    const app = apps.get(appId);
    if (!app) {
      lastUsed++;
      layout.push({ id: nextEmptyId++, app: null, startChannel: idx });
      return;
    }
    lastUsed = idx + Number(channels) - 1;
    layout.push({ id: layoutId, app, startChannel: idx });
  });

  return layout;
};
