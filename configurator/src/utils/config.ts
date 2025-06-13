import { Layout } from "@atov/fp-config";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "./usb-protocol";

export const setLayout = async (dev: USBDevice, layout: Array<number>) => {
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

  for (let i = 0; i < Math.min(layout.length, 16); i++) {
    if (layout[i]) {
      send_layout[0][i] = [layout[i], BigInt(0)];
    }
  }

  await sendMessage(dev, {
    tag: "SetGlobalConfig",
    value: {
      clock_src: { tag: "MidiIn" },
      reset_src: { tag: "MidiIn" },
      layout: send_layout,
    },
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

export const getState = async (dev: USBDevice) => {
  const result = await sendAndReceive(dev, {
    tag: "GetState",
  });

  if (result.tag === "BatchMsgStart") {
    return receiveBatchMessages(dev, result.value);
  }
};
