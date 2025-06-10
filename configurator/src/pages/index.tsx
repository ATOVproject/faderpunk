import { useCallback, useState } from "react";
import { Button } from "@heroui/button";
import { Chip } from "@heroui/chip";
import { Form } from "@heroui/form";
import {
  Table,
  TableHeader,
  TableColumn,
  TableBody,
  TableRow,
  TableCell,
} from "@heroui/table";
import { button as buttonStyles } from "@heroui/theme";

import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";
import { connectToFaderPunk, getDeviceName } from "@/utils/usb-protocol";
import { getAllApps, setLayout } from "@/utils/config";

// TODO: Load all available apps including their possible configurations from the device
export default function IndexPage() {
  const [usbDevice, setUsbDevice] = useState<USBDevice | null>(null);
  const [apps, setApps] = useState<
    {
      appId: string;
      channels: string;
      name: string;
      description: string;
      paramCount: string;
    }[]
  >([]);
  const [selectedApps, setSelectedApps] = useState<string[]>([]);

  const handleConnectToFaderPunk = useCallback(async () => {
    try {
      const device = await connectToFaderPunk();

      setUsbDevice(device);

      const appsData = await getAllApps(device);
      // const layout = await getLayout(device);
      // console.log(layout);

      if (appsData) {
        // Parse apps data into the expected format
        const parsedApps = appsData
          .filter(
            (item): item is Extract<typeof item, { tag: "AppConfig" }> =>
              item.tag === "AppConfig",
          )
          .map((app) => ({
            appId: app.value[0].toString(),
            channels: app.value[1].toString(),
            paramCount: app.value[2][0].toString(),
            name: app.value[2][1] as string,
            description: app.value[2][2] as string,
          }));

        setApps(parsedApps);
      }

      // await sendMessage(device, {
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
    } catch (error) {
      console.error("Failed to connect to Fader Punk:", error);
    }
  }, []);

  const handleAddApp = useCallback((appId: string) => {
    setSelectedApps((prev) => {
      if (prev.length >= 16) {
        return prev;
      }

      return [...prev, appId];
    });
  }, []);

  const handleRemoveApp = useCallback((index: number) => {
    setSelectedApps((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const deviceName = usbDevice ? getDeviceName(usbDevice) : "";

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
            onPress={handleConnectToFaderPunk}
          >
            Connect to Fader Punk
          </Button>
        ) : (
          <Form
            className="flex flex-col items-start gap-2"
            validationBehavior="native"
          >
            <span>Connected to {deviceName}</span>
            {apps && apps.length > 0 && (
              <div className="w-full max-w-4xl">
                <h2 className={title({ size: "sm" })}>Available Apps</h2>
                <Table aria-label="Available apps table" className="mt-4">
                  <TableHeader>
                    <TableColumn>APP ID</TableColumn>
                    <TableColumn>CHANNELS</TableColumn>
                    <TableColumn>NAME</TableColumn>
                    <TableColumn>DESCRIPTION</TableColumn>
                    <TableColumn>PARAMETERS</TableColumn>
                    <TableColumn>ACTIONS</TableColumn>
                  </TableHeader>
                  <TableBody>
                    {apps.map((app, index) => (
                      <TableRow key={app.appId || index}>
                        <TableCell>{app.appId}</TableCell>
                        <TableCell>{app.channels}</TableCell>
                        <TableCell>{app.name}</TableCell>
                        <TableCell>{app.description}</TableCell>
                        <TableCell>{app.paramCount}</TableCell>
                        <TableCell>
                          <Button
                            isDisabled={selectedApps.length >= 16}
                            size="sm"
                            variant="flat"
                            onPress={() => handleAddApp(app.appId)}
                          >
                            Add
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}
            {selectedApps.length > 0 && (
              <div className="w-full max-w-4xl">
                <h2 className={title({ size: "sm" })}>Selected Apps</h2>
                <div className="flex flex-wrap gap-2 mt-4">
                  {selectedApps.map((appId, index) => (
                    <Chip
                      key={index}
                      color="primary"
                      variant="flat"
                      onClose={() => handleRemoveApp(index)}
                    >
                      App ID: {appId}
                    </Chip>
                  ))}
                </div>
              </div>
            )}
            <Button
              disabled={!selectedApps.length}
              type="button"
              variant="bordered"
              onPress={() => setLayout(usbDevice, selectedApps.map(Number))}
            >
              Set Layout
            </Button>
          </Form>
        )}
      </section>
    </DefaultLayout>
  );
}
