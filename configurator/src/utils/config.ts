import { ClockSrc, I2cMode, Layout } from "@atov/fp-config";

import { ParsedApp } from "../pages";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "./usb-protocol";

export const setGlobalConfig = async (
  dev: USBDevice,
  clock_src: ClockSrc,
  reset_src: ClockSrc,
  i2c_mode: I2cMode,
) => {
  await sendMessage(dev, {
    tag: "SetGlobalConfig",
    value: {
      clock: {
        clock_src,
        reset_src,
        internal_bpm: 120,
      },
      i2c_mode,
      quantizer_key: { tag: "PentatonicMajor" },
      quantizer_tonic: { tag: "C" },
      led_brightness: 150,
    },
  });
};

export const setLayout = async (
  dev: USBDevice,
  layout: Array<number>,
  allApps: Map<number, ParsedApp>,
) => {
  let send_layout: Layout = [
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

        if (current_chan + parseInt(channels, 10) > 16) {
          break;
        }
        send_layout[0][current_chan] = [layout[i], BigInt(channels)];
        current_chan += parseInt(channels, 10);
      }
    }
  }

  await sendMessage(dev, {
    tag: "SetLayout",
    value: send_layout,
  });
};

export const getAllApps = async (dev: USBDevice) => {
  const result = await sendAndReceive(dev, {
    tag: "GetAllApps",
  });

  if (result.tag === "BatchMsgStart") {
    return receiveBatchMessages(dev, result.value);
  }
};

export const getAppParams = async (dev: USBDevice, startChannel: string) => {
  return sendAndReceive(dev, {
    tag: "GetAppParams",
    value: { start_channel: BigInt(startChannel) },
  });
};

export const setAppParams = async (
  dev: USBDevice,
  startChannel: number,
  paramValues: any[],
) => {
  // Transform form values to Value enum format
  const transformValue = (paramValue: any) => {
    if (!paramValue || paramValue.tag === "None") {
      return undefined;
    }

    switch (paramValue.tag) {
      case "i32":
        return { tag: "i32", value: paramValue.value };
      case "f32":
        return { tag: "f32", value: paramValue.value };
      case "bool":
        return { tag: "bool", value: paramValue.value };
      case "Enum":
        return { tag: "Enum", value: paramValue.value };
      case "Curve":
        return { tag: "Curve", value: paramValue.value };
      case "Waveform":
        return { tag: "Waveform", value: paramValue.value };
      case "Color":
        return { tag: "Color", value: paramValue.value };
      default:
        return undefined;
    }
  };

  // Create fixed-length tuple of 8 values (APP_MAX_PARAMS = 8)
  const values: [any, any, any, any, any, any, any, any] = [
    paramValues.length > 0 ? transformValue(paramValues[0]) : undefined,
    paramValues.length > 1 ? transformValue(paramValues[1]) : undefined,
    paramValues.length > 2 ? transformValue(paramValues[2]) : undefined,
    paramValues.length > 3 ? transformValue(paramValues[3]) : undefined,
    paramValues.length > 4 ? transformValue(paramValues[4]) : undefined,
    paramValues.length > 5 ? transformValue(paramValues[5]) : undefined,
    paramValues.length > 6 ? transformValue(paramValues[6]) : undefined,
    paramValues.length > 7 ? transformValue(paramValues[7]) : undefined,
  ];

  return sendMessage(dev, {
    tag: "SetAppParams",
    value: {
      start_channel: BigInt(startChannel),
      values: values,
    },
  });
};

export const getGlobalConfig = async (dev: USBDevice) => {
  return sendAndReceive(dev, {
    tag: "GetGlobalConfig",
  });
};

export const getLayout = async (dev: USBDevice) => {
  return sendAndReceive(dev, {
    tag: "GetLayout",
  });
};
