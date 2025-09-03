import { useCallback, useState } from "react";
import { Button } from "@heroui/button";
import { Chip } from "@heroui/chip";
import { Form } from "@heroui/form";
import { Select, SelectItem } from "@heroui/select";
import {
  Table,
  TableHeader,
  TableColumn,
  TableBody,
  TableRow,
  TableCell,
} from "@heroui/table";
import { button as buttonStyles } from "@heroui/theme";
import { ClockSrc, I2cMode, Param } from "@atov/fp-config";

import { AppConfigDrawer } from "@/components/app-config-drawer";
import { title } from "@/components/primitives";
import DefaultLayout from "@/layouts/default";
import { connectToFaderPunk, getDeviceName } from "@/utils/usb-protocol";
import {
  getAllApps,
  getGlobalConfig,
  getLayout,
  setGlobalConfig,
  setLayout,
} from "@/utils/config";

export type ParsedApp = {
  appId: number;
  channels: string;
  name: string;
  description: string;
  paramCount: string;
  params: Param[];
};

// TODO: Load all available apps including their possible configurations from the device
export default function IndexPage() {
  const [usbDevice, setUsbDevice] = useState<USBDevice | null>(null);
  const [apps, setApps] = useState<Map<number, ParsedApp>>(new Map());
  const [selectedApps, setSelectedApps] = useState<
    { appId: number; startChannel: number }[]
  >([]);
  const [clockSrc, setClockSrc] = useState<ClockSrc>({ tag: "Internal" });
  const [resetSrc, setResetSrc] = useState<ClockSrc>({ tag: "Internal" });
  const i2cMode: I2cMode = { tag: "Follower" };
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [selectedAppForConfig, setSelectedAppForConfig] = useState<{
    appId: number;
    startChannel: number;
  } | null>(null);

  const handleConnectToFaderPunk = useCallback(async () => {
    try {
      const device = await connectToFaderPunk();

      setUsbDevice(device);

      const appsData = await getAllApps(device);
      const globalConfig = await getGlobalConfig(device);
      const layout = await getLayout(device);

      if (appsData) {
        // Parse apps data into a Map for easy lookup by app ID
        const parsedApps = new Map<number, ParsedApp>();

        appsData
          .filter(
            (item): item is Extract<typeof item, { tag: "AppConfig" }> =>
              item.tag === "AppConfig",
          )
          .forEach((app) => {
            const appConfig = {
              appId: app.value[0],
              channels: app.value[1].toString(),
              paramCount: app.value[2][0].toString(),
              name: app.value[2][1] as string,
              description: app.value[2][2] as string,
              params: app.value[2][3],
            };

            parsedApps.set(appConfig.appId, appConfig);
          });

        setApps(parsedApps);
      }

      if (globalConfig && globalConfig.tag === "GlobalConfig") {
        setClockSrc(globalConfig.value.clock.clock_src);
        setResetSrc(globalConfig.value.clock.reset_src);
      }

      if (layout && layout.tag == "Layout") {
        const appsWithChannels = layout.value[0]
          .map((app_data, index) =>
            app_data ? { appId: app_data[0], startChannel: index } : null,
          )
          .filter((app) => app !== null) as {
          appId: number;
          startChannel: number;
        }[];

        setSelectedApps(appsWithChannels);
      }
    } catch (error) {
      console.error("Failed to connect to Faderpunk:", error);
    }
  }, []);

  const handleAddApp = useCallback((appId: number) => {
    setSelectedApps((prev) => {
      if (prev.length >= 16) {
        return prev;
      }

      // Find the next available start channel
      const usedChannels = new Set(prev.map((app) => app.startChannel));
      let startChannel = 0;

      while (usedChannels.has(startChannel) && startChannel < 16) {
        startChannel++;
      }

      if (startChannel >= 16) {
        return prev;
      }

      return [...prev, { appId, startChannel }];
    });
  }, []);

  const handleRemoveApp = useCallback((index: number) => {
    setSelectedApps((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handleChipClick = useCallback(
    (app: { appId: number; startChannel: number }) => {
      setSelectedAppForConfig(app);
      setIsDrawerOpen(true);
    },
    [],
  );

  const handleDrawerClose = useCallback(() => {
    setIsDrawerOpen(false);
    setSelectedAppForConfig(null);
  }, []);

  const deviceName = usbDevice ? getDeviceName(usbDevice) : "";

  return (
    <DefaultLayout>
      <section className="flex flex-col items-center justify-center gap-4 py-8 md:py-10">
        <div className="inline-block max-w-lg text-center justify-center">
          <span className={title()}>Configure&nbsp;</span>
          <span className={title({ color: "yellow" })}>Faderpunk&nbsp;</span>
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
            Connect to Faderpunk
          </Button>
        ) : (
          <Form
            className="flex flex-col items-start gap-2"
            validationBehavior="native"
          >
            <span>Connected to {deviceName}</span>
            {apps && apps.size > 0 && (
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
                    {Array.from(apps.values()).map((app, index) => (
                      <TableRow key={app.appId || index}>
                        <TableCell>{app.appId}</TableCell>
                        <TableCell>{app.channels}</TableCell>
                        <TableCell>{app.name}</TableCell>
                        <TableCell>{app.description}</TableCell>
                        <TableCell>{app.params.length}</TableCell>
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
                  {selectedApps.map((app, index) => (
                    <Chip
                      key={index}
                      className="cursor-pointer"
                      color="primary"
                      variant="flat"
                      onClick={() => handleChipClick(app)}
                      onClose={() => handleRemoveApp(index)}
                    >
                      App {app.appId} (Ch {app.startChannel + 1})
                    </Chip>
                  ))}
                </div>
              </div>
            )}
            <Button
              disabled={!selectedApps.length}
              type="button"
              variant="bordered"
              onPress={async () => {
                await setLayout(
                  usbDevice,
                  selectedApps.map((app) => app.appId),
                  apps,
                );
                let layout = await getLayout(usbDevice);

                if (layout && layout.tag == "Layout") {
                  const appsWithChannels = layout.value[0]
                    .map((app_data, index) =>
                      app_data
                        ? { appId: app_data[0], startChannel: index }
                        : null,
                    )
                    .filter((app) => app !== null) as {
                    appId: number;
                    startChannel: number;
                  }[];

                  setSelectedApps(appsWithChannels);
                }
              }}
            >
              Set layout
            </Button>
            <div className="w-full max-w-4xl">
              <h2 className={title({ size: "sm" })}>Device config</h2>
              <div className="flex gap-4 items-end mt-4">
                <Select
                  className="flex-1"
                  label="Clock Source"
                  placeholder="Select clock source"
                  selectedKeys={[clockSrc.tag]}
                  onSelectionChange={(keys) => {
                    const key = Array.from(keys)[0] as string;

                    setClockSrc({ tag: key } as ClockSrc);
                  }}
                >
                  <SelectItem key="None">None</SelectItem>
                  <SelectItem key="Atom">Atom</SelectItem>
                  <SelectItem key="Meteor">Meteor</SelectItem>
                  <SelectItem key="Cube">Cube</SelectItem>
                  <SelectItem key="Internal">Internal</SelectItem>
                  <SelectItem key="MidiIn">MIDI In</SelectItem>
                  <SelectItem key="MidiUsb">MIDI USB</SelectItem>
                </Select>
                <Select
                  className="flex-1"
                  label="Reset Source"
                  placeholder="Select reset source"
                  selectedKeys={[resetSrc.tag]}
                  onSelectionChange={(keys) => {
                    const key = Array.from(keys)[0] as string;

                    setResetSrc({ tag: key } as ClockSrc);
                  }}
                >
                  <SelectItem key="None">None</SelectItem>
                  <SelectItem key="Atom">Atom</SelectItem>
                  <SelectItem key="Meteor">Meteor</SelectItem>
                  <SelectItem key="Cube">Cube</SelectItem>
                  <SelectItem key="Internal">Internal</SelectItem>
                  <SelectItem key="MidiIn">MIDI In</SelectItem>
                  <SelectItem key="MidiUsb">MIDI USB</SelectItem>
                </Select>
              </div>
            </div>
            <Button
              type="button"
              variant="bordered"
              onPress={() =>
                setGlobalConfig(usbDevice, clockSrc, resetSrc, i2cMode)
              }
            >
              Set config
            </Button>
          </Form>
        )}
      </section>

      {usbDevice ? (
        <AppConfigDrawer
          appConfig={
            selectedAppForConfig
              ? apps.get(selectedAppForConfig.appId) || null
              : null
          }
          isOpen={isDrawerOpen}
          selectedApp={selectedAppForConfig}
          usbDevice={usbDevice}
          onClose={handleDrawerClose}
        />
      ) : null}
    </DefaultLayout>
  );
}
