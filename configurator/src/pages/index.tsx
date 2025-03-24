import { useCallback } from "react";
import { Button } from "@heroui/button";
import { Form } from "@heroui/form";
import { useState } from "react";
import { button as buttonStyles } from "@heroui/theme";
import { deserialize, Param } from "@atov/fp-config";

import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";

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

    await usbDevice?.transferOut(1, new Uint8Array([1]));
    const data = await usbDevice?.transferIn(1, 256);

    if (data?.data?.buffer) {
      const dataBuf = new Uint8Array(data.data.buffer);
      const cobsDecoded = cobsDecode(dataBuf.slice(0, dataBuf.length - 1));

      const len = (cobsDecoded[0] << 8) | cobsDecoded[1];

      const postcardDecoded = deserialize("ConfigMsgOut", cobsDecoded.slice(2));

      if (postcardDecoded.value.tag === "AppList") {
        let availableApps = postcardDecoded.value.value;

        setApps(availableApps);
      }
    }
  }, []);

  const deviceName = `${usbDevice?.manufacturerName} ${usbDevice?.productName} v${usbDevice?.deviceVersionMajor}.${usbDevice?.deviceVersionMinor}.${usbDevice?.deviceVersionSubminor}`;

  return (
    <DefaultLayout>
      <section className="flex flex-col items-center justify-center gap-4 py-8 md:py-10">
        <div className="inline-block max-w-lg text-center justify-center">
          <span className={title()}>Configure&nbsp;</span>
          <span className={title({ color: "violet" })}>Fader Punk&nbsp;</span>
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
