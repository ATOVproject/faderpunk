import { useCallback } from "react";
import { Button } from "@heroui/button";
import { Form } from "@heroui/form";
import { useState } from "react";
import { button as buttonStyles } from "@heroui/theme";
import {
  ConfigMsgIn,
  ConfigMsgOut,
  deserialize,
  Param,
  serialize,
} from "@atov/fp-config";

import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";

const FRAME_DELIMITER = 0;

type ValidParam = Exclude<Param, { tag: "None" }>;

export function cobsEncode(data: Uint8Array): Uint8Array {
  // Allocate output buffer with worst-case size
  const maxSize = data.length + Math.ceil(data.length / 254) + 1;
  const encoded = new Uint8Array(maxSize);

  let codeIndex = 0; // Index where we'll write the current code byte
  let writeIndex = 1; // Start writing data at position 1
  let code = 1; // Current code value, starts at 1

  // Process each input byte
  for (let i = 0; i < data.length; i++) {
    if (data[i] === 0) {
      // Zero byte found, write the code and reset
      encoded[codeIndex] = code;
      code = 1;
      codeIndex = writeIndex++;
    } else {
      // Non-zero byte, copy it to output
      encoded[writeIndex++] = data[i];
      code++;

      // If we've reached the maximum code value, write code and start a new block
      if (code === 255) {
        encoded[codeIndex] = code;
        code = 1;
        codeIndex = writeIndex++;
      }
    }
  }

  // Write the final code byte
  encoded[codeIndex] = code;

  // Return the actual encoded data
  return encoded.slice(0, writeIndex);
}

export function cobsDecode(data: Uint8Array): Uint8Array {
  if (data.length === 0) {
    return new Uint8Array(0);
  }

  // Allocate output buffer
  const decoded = new Uint8Array(data.length);
  let writeIndex = 0;
  let readIndex = 0;

  while (readIndex < data.length) {
    // Read the code byte
    const code = data[readIndex++];

    if (code === 0) {
      throw new Error("Invalid COBS-encoded data: zero code byte found");
    }

    // Copy data bytes
    for (let i = 1; i < code; i++) {
      if (readIndex >= data.length) {
        break; // End of input reached
      }
      decoded[writeIndex++] = data[readIndex++];
    }

    // Unless this was the last block or the code was 255, add a zero byte
    if (readIndex < data.length && code < 255) {
      decoded[writeIndex++] = 0;
    }
  }

  // Return the actual decoded data
  return decoded.slice(0, writeIndex);
}

const punkOneShot = async (usbDevice: USBDevice, msg: ConfigMsgIn) => {
  const serialized = serialize("ConfigMsgIn", msg);
  const buf = new Uint8Array(serialized.length + 2);

  buf[0] = (serialized.length >> 8) & 0xff;
  buf[1] = serialized.length & 0xff;
  buf.set(serialized, 2);

  const cobsResult = cobsEncode(buf);
  const cobsEncoded = new Uint8Array(cobsResult.length + 1);

  cobsEncoded.set(cobsResult, 0);
  cobsEncoded[cobsEncoded.length] = FRAME_DELIMITER;

  return usbDevice?.transferOut(1, cobsEncoded);
};

const punkRequest = async (usbDevice: USBDevice, msg: ConfigMsgIn) => {
  await punkOneShot(usbDevice, msg);

  return receiveMessage(usbDevice);
};

const receiveMessage = async (usbDevice: USBDevice): Promise<ConfigMsgOut> => {
  const data = await usbDevice?.transferIn(1, 512);

  if (!data?.data?.buffer) {
    throw new Error("No data received");
  }

  const dataBuf = new Uint8Array(data.data.buffer);
  const cobsDecoded = cobsDecode(dataBuf.slice(0, dataBuf.length - 1));

  // const len = (cobsDecoded[0] << 8) | cobsDecoded[1];

  let res = deserialize("ConfigMsgOut", cobsDecoded.slice(2));

  return res.value;
};

const receiveBatchMessages = async (usbDevice: USBDevice, count: bigint) => {
  let resArray: Promise<ConfigMsgOut>[] = [];

  for (let i = 0; i < count; i++) {
    resArray.push(receiveMessage(usbDevice));
  }
  let results = await Promise.all(resArray);
  let last = await receiveMessage(usbDevice);

  if (last.tag !== "BatchMsgEnd") {
    throw new Error("Unexpected message in batch end.");
  }

  return results;
};

// TODO: Load all available apps including their possible configurations from the device
export default function IndexPage() {
  const [usbDevice, setUsbDevice] = useState<USBDevice | null>(null);
  const [apps /* ,setApps */] = useState<
    {
      name: string;
      description: string;
      params: ValidParam[];
    }[]
  >();

  const connectToFaderPunk = useCallback(async () => {
    const usbDevice = await navigator.usb.requestDevice({
      filters: [{ vendorId: 0xf569, productId: 0x1 }],
    });

    await usbDevice.open();

    await usbDevice.claimInterface(1);
    setUsbDevice(usbDevice);

    let result = await punkRequest(usbDevice, {
      tag: "GetLayout",
    });

    if (result.tag === "BatchMsgStart") {
      const results = await receiveBatchMessages(usbDevice, result.value);

      console.log(results);

      // await punkOneShot(usbDevice, {
      //   tag: "SetAppParam",
      //   value: [
      //     BigInt(0),
      //     BigInt(0),
      //     { tag: "Curve", value: { tag: "Logarithmic" } },
      //   ],
      // });

      // const appConfigs = results
      //   .filter(
      //     (res): res is Extract<ConfigMsgOut, { tag: "AppConfig" }> =>
      //       res.tag === "AppConfig",
      //   )
      //   .map(({ value }) => ({
      //     name: value[0],
      //     description: value[1],
      //     params: value[2] as ValidParam[],
      //   }));
      //
      // setApps(appConfigs);
    }
  }, []);

  const setLayoutAllDefault = useCallback(async () => {
    if (!usbDevice) {
      return;
    }
    console.log("Setting layout to default");
    await punkOneShot(usbDevice, {
      tag: "SetLayout",
      value: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    });
  }, [usbDevice]);

  const setLayoutMixed = useCallback(async () => {
    if (!usbDevice) {
      return;
    }
    console.log("Setting layout to mixed");
    await punkOneShot(usbDevice, {
      tag: "SetLayout",
      value: [1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2],
    });
  }, [usbDevice]);

  const deviceName = `${usbDevice?.manufacturerName} ${usbDevice?.productName} v${usbDevice?.deviceVersionMajor}.${usbDevice?.deviceVersionMinor}.${usbDevice?.deviceVersionSubminor}`;

  return (
    <DefaultLayout>
      <section className="flex flex-col items-center justify-center gap-4 py-8 md:py-10">
        <div className="inline-block max-w-lg text-center justify-center">
          <span className={title()}>Configure&nbsp;</span>
          <span className={title({ color: "yellow" })}>Fader Punk&nbsp;</span>
        </div>

        {!usbDevice ? (
          <Button
            className={buttonStyles({
              color: "primary",
              radius: "full",
              variant: "shadow",
            })}
            onPress={connectToFaderPunk}
          >
            Connect to Fader Punk
          </Button>
        ) : (
          <Form
            className="flex flex-col items-start gap-2"
            validationBehavior="native"
          >
            <span>Connected to {deviceName}</span>
            {apps && apps.length && (
              <div>
                <h2 className={title({ size: "sm" })}>Available apps</h2>
                <ul>
                  {apps.map((app) => (
                    <li key={app.name} className="mb-2">
                      <span>
                        {app.name} - {app.description}
                      </span>
                      <br />
                      {app.params.length ? (
                        <span className="ml-1 text-small">
                          Parameters:{" "}
                          {app.params
                            .map(
                              (param) => `${param.value.name} (${param.tag})`,
                            )
                            .join(",")}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <Button
              type="button"
              variant="bordered"
              onPress={setLayoutAllDefault}
            >
              Set Layout all default
            </Button>
            <Button type="button" variant="bordered" onPress={setLayoutMixed}>
              Set Layout mixed
            </Button>
          </Form>
        )}
      </section>
    </DefaultLayout>
  );
}
