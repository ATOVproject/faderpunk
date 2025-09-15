import type { ClockSrc, I2cMode, Layout, AuxJackMode } from "@atov/fp-config";

import type { AllApps, App, AppLayout } from "../utils/types";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "../utils/usb-protocol";
import { transformParamValues } from "./utils";

export const setGlobalConfig = async (
  dev: USBDevice,
  clock_src: ClockSrc,
  reset_src: ClockSrc,
  aux: [AuxJackMode, AuxJackMode, AuxJackMode],
  i2c_mode: I2cMode,
) => {
  await sendMessage(dev, {
    tag: "SetGlobalConfig",
    value: {
      aux,
      clock: {
        clock_src,
        ext_ppqn: 24,
        reset_src,
        internal_bpm: 120,
      },
      i2c_mode,
      quantizer: {
        key: { tag: "PentatonicMaj" },
        tonic: { tag: "C" },
      },
      led_brightness: 150,
    },
  });
};

export const setLayout = async (
  dev: USBDevice,
  layout: Array<number>,
  allApps: Map<number, App>,
) => {
  const send_layout: Layout = [
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

  let current_chan = 0;

  for (let i = 0; i < Math.min(layout.length, 16); i++) {
    if (layout[i]) {
      const app = allApps.get(layout[i]);

      if (app) {
        const { channels } = app;

        if (current_chan + channels > 16) {
          break;
        }
        send_layout[0][current_chan] = [layout[i], BigInt(channels)];
        current_chan += channels;
      }
    }
  }

  await sendMessage(dev, {
    tag: "SetLayout",
    value: send_layout,
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
        id: app.value[0],
        channels: app.value[1],
        paramCount: app.value[2][0],
        name: app.value[2][1] as string,
        description: app.value[2][2] as string,
        color: app.value[2][3].tag,
        icon: app.value[2][4].tag,
        params: app.value[2][5],
      };

      parsedApps.set(appConfig.id, appConfig);
    });

  return parsedApps;
};

export const getAppParams = async (dev: USBDevice, startChannel: number) => {
  const response = await sendAndReceive(dev, {
    tag: "GetAppParams",
    value: { start_channel: BigInt(startChannel) },
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
  startChannel: number,
  paramValues: Record<string, string | boolean>,
) => {
  const values = transformParamValues(paramValues);
  return sendMessage(dev, {
    tag: "SetAppParams",
    value: {
      start_channel: BigInt(startChannel),
      values,
    },
  });
};

export const getGlobalConfig = async (dev: USBDevice) => {
  return sendAndReceive(dev, {
    tag: "GetGlobalConfig",
  });
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

  let i = 0;
  let lastUsed = -1;

  while (i < 16) {
    const app = response.value[0][i];
    if (!app) {
      if (i > lastUsed) {
        layout.push({ slotNumber: i });
        lastUsed++;
      }
    } else {
      const appData = apps.get(app[0]);
      if (!appData) {
        layout.push({ slotNumber: i });
        lastUsed++;
      } else {
        const end = i + Number(appData.channels) - 1;
        layout.push({ ...appData, start: i, end });
        lastUsed = end;
      }
    }
    i++;
  }

  return layout.slice(0, 15);
};
