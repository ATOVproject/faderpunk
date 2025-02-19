import { useCallback } from "react";
import { InputOtp } from "@heroui/input-otp";
import { Input } from "@heroui/input";
import { Button } from "@heroui/button";
import { Form } from "@heroui/form";
import { FormEvent, useState } from "react";
import { button as buttonStyles } from "@heroui/theme";

import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";

// TODO: Load all available apps including their possible configurations from the device
export default function IndexPage() {
  const [usbDevice, setUsbDevice] = useState<USBDevice | null>(null);
  const [size, setSize] = useState("5");
  const [apps, setApps] = useState("");
  const [deviceApps, setDeviceApps] = useState<number[] | null>(null);

  const connectToFaderPunk = useCallback(async () => {
    const device = await navigator.usb.requestDevice({
      filters: [{ vendorId: 0xf569, productId: 0x1 }],
    });

    await device.open();

    await device.claimInterface(1);

    setUsbDevice(device);
  }, []);

  const sendApps = useCallback(
    async (ev: FormEvent<HTMLFormElement>) => {
      ev.preventDefault();
      setDeviceApps(null);
      const appArr = Array.from(apps).map(Number);

      await usbDevice?.transferOut(1, new Uint8Array(appArr));
      const data = await usbDevice?.transferIn(1, 64);

      if (data?.data?.buffer) {
        const dataBuf = new Uint8Array(data.data.buffer);

        setDeviceApps(Array.from(dataBuf));
      }
    },
    [apps],
  );

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
            onSubmit={sendApps}
          >
            <span>Connected to {deviceName}</span>
            <Input
              label="Number of apps"
              labelPlacement="outside"
              max={16}
              min={1}
              type="number"
              value={size}
              onValueChange={setSize}
            />
            <div className="text-small">App numbers</div>
            <InputOtp
              required
              label="Apps"
              length={parseInt(size)}
              value={apps}
              onValueChange={setApps}
            />
            <Button type="submit" variant="bordered">
              Submit
            </Button>
            {deviceApps && (
              <div className="text-small text-default-500">
                Set device apps to <code>{JSON.stringify(deviceApps)}</code>
              </div>
            )}
          </Form>
        )}
      </section>
    </DefaultLayout>
  );
}
