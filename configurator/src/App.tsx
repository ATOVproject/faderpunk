import { useState } from "react";
import { Tabs, Tab } from "@heroui/tabs";

import { ButtonPrimary } from "./components/Button";
import { Icon } from "./components/Icon";
import { Layout } from "./components/Layout";
import { useStore } from "./store";
import { DeviceTab } from "./components/DeviceTab";
import { AppsTab } from "./components/AppsTab";
import { SettingsTab } from "./components/SettingsTab";

const App = () => {
  const { apps, config, layout, usbDevice, connect } = useStore();
  const [modalApp, setModalApp] = useState<number | null>(null);

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

      <Tabs
        className="border-default-100 mb-8 w-full border-b-3"
        classNames={{
          tabList: "flex p-0 rounded-none gap-0",
          cursor: "rounded-none rounded-t-md dark:bg-black",
          tab: "px-12 py-6",
          tabContent: "text-white font-bold uppercase text-lg",
        }}
        variant="light"
      >
        <Tab key="device" title="Device">
          <DeviceTab layout={layout} setModalApp={setModalApp} />
        </Tab>
        <Tab key="apps" title="Apps">
          <AppsTab apps={apps} layout={layout} setModalApp={setModalApp} />
        </Tab>
        <Tab key="settings" title="Settings">
          <SettingsTab config={config} />
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
