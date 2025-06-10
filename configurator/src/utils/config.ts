import { FixedLengthArray, u8 } from "@atov/fp-config";

import {
  receiveBatchMessages,
  sendAndReceive,
  sendMessage,
} from "./usb-protocol";

export const setLayout = async (dev: USBDevice, layout: Array<number>) => {
  const fixedLengthLayout = new Array(16).fill(0);

  for (let i = 0; i < Math.min(layout.length, 16); i++) {
    fixedLengthLayout[i] = layout[i];
  }
  await sendMessage(dev, {
    tag: "SetLayout",
    value: fixedLengthLayout as unknown as FixedLengthArray<u8, 16>,
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

export const getLayout = async (dev: USBDevice) => {
  const result = await sendAndReceive(dev, {
    tag: "GetLayout",
  });

  if (result.tag === "BatchMsgStart") {
    return receiveBatchMessages(dev, result.value);
  }
};
