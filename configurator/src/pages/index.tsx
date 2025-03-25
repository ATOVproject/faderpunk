import { useCallback } from "react";
import { Button } from "@heroui/button";
import { Form } from "@heroui/form";
import { useState } from "react";
import { button as buttonStyles } from "@heroui/theme";
import { ConfigMsgIn, deserialize, Param, serialize } from "@atov/fp-config";

import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";

const FRAME_DELIMITER = 0;

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

const createMessage = (msg: ConfigMsgIn) => {
  const serialized = serialize("ConfigMsgIn", msg);
  const buf = new Uint8Array(serialized.length + 2);

  buf[0] = (serialized.length >> 8) & 0xff;
  buf[1] = serialized.length & 0xff;
  buf.set(serialized, 2);

  const cobsResult = cobsEncode(buf);
  const cobsEncoded = new Uint8Array(cobsResult.length + 1);

  cobsEncoded.set(cobsResult, 0);
  cobsEncoded[cobsEncoded.length] = FRAME_DELIMITER;

  return cobsEncoded;
};

// TODO: Load all available apps including their possible configurations from the device
export default function IndexPage() {
  const [usbDevice, setUsbDevice] = useState<USBDevice | null>(null);
  const [apps, setApps] = useState<[string, string, Param[]][]>();

  const connectToFaderPunk = useCallback(async () => {
    const usbDevice = await navigator.usb.requestDevice({
      filters: [{ vendorId: 0xf569, productId: 0x1 }],
    });

    await usbDevice.open();

    await usbDevice.claimInterface(1);
    setUsbDevice(usbDevice);

    let msg = createMessage({
      tag: "GetApps",
    });

    await usbDevice?.transferOut(1, msg);
    const data = await usbDevice?.transferIn(1, 256);

    if (data?.data?.buffer) {
      const dataBuf = new Uint8Array(data.data.buffer);
      const cobsDecoded = cobsDecode(dataBuf.slice(0, dataBuf.length - 1));

      const len = (cobsDecoded[0] << 8) | cobsDecoded[1];

      const postcardDecoded = deserialize("ConfigMsgOut", cobsDecoded.slice(2));

      console.log(postcardDecoded);

      // if (postcardDecoded.value.tag === "AppList") {
      //   let availableApps = postcardDecoded.value.value;
      //
      //   setApps(availableApps);
      // }
    }
  }, []);

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
                    <li key={app[0]}>
                      {app[0]} - {app[1]}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {/* <Button type="submit" variant="bordered"> */}
            {/*   Submit */}
            {/* </Button> */}
          </Form>
        )}
      </section>
    </DefaultLayout>
  );
}
