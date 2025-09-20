import { useState } from "react";
import { Tabs, Tab } from "@heroui/tabs";
import classNames from "classnames";

import { ButtonPrimary } from "./components/Button";
import { ChannelOverview } from "./components/app-layout/ChannelOverview";
import { Icon } from "./components/Icon";
import { Layout } from "./components/Layout";
import { useStore } from "./store";
import { DeviceTab } from "./components/DeviceTab";
import { AppsTab } from "./components/AppsTab";

const App = () => {
  const { usbDevice, layout, apps, connect } = useStore();
  const [modalApp, setModalApp] = useState<number | null>(null);

  const [selectedTab, setSelectedTab] = useState<
    "device" | "apps" | "settings"
  >("device");

  if (!usbDevice) {
    return (
      <Layout
        onModalOpenChange={(isOpen) => setModalApp(isOpen ? -1 : null)}
        modalApp={modalApp}
      >
        <ButtonPrimary onPress={connect}>Connect</ButtonPrimary>
      </Layout>
    );
  }

  return (
    <Layout
      modalApp={modalApp}
      onModalOpenChange={(isOpen) => setModalApp(isOpen ? -1 : null)}
    >
      <div className="mb-8 text-center">
        <img src="/img/fp-logo.svg" className="inline w-64" />
        <p className="font-vox mt-3 text-xl font-semibold tracking-wider text-white uppercase">
          Configurator
        </p>
      </div>

      <div className="border-default-100 mb-8 flex border-b-3">
        <button
          className={classNames(
            "cursor-pointer rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase",
            {
              "bg-black": selectedTab == "device",
            },
          )}
          onClick={() => setSelectedTab("device")}
        >
          Device
        </button>
        <button
          className={classNames(
            "cursor-pointer rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase",
            {
              "bg-black": selectedTab == "apps",
            },
          )}
          onClick={() => setSelectedTab("apps")}
        >
          Apps
        </button>
        <button
          className={classNames(
            "cursor-pointer rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase",
            {
              "bg-black": selectedTab == "settings",
            },
          )}
          onClick={() => setSelectedTab("settings")}
        >
          Settings
        </button>
      </div>

      <div className="mb-12">
        <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
          Channel Overview
        </h2>
        <ChannelOverview onClick={() => setModalApp(-1)} layout={layout} />
      </div>

      <Tabs classNames={{ base: "hidden" }} selectedKey={selectedTab}>
        <Tab key="device" title="Device">
          <DeviceTab layout={layout} />
        </Tab>
        <Tab key="apps" title="Apps">
          <AppsTab apps={apps} setModalApp={setModalApp} />
        </Tab>
      </Tabs>

      <div className="border-default-100 mt-16 border-t-3">
        <div className="flex items-center justify-between py-8">
          <img src="/img/atov-logo.svg" className="w-16" />
          <div className="flex items-center gap-4">
            <Icon className="h-6 w-6" name="discord" />
            <Icon className="h-6 w-6" name="github" />
            <Icon className="h-6 w-6" name="instagram" />
          </div>
        </div>
      </div>
    </Layout>
  );
};

export default App;
