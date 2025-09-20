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
      <main className="flex min-h-screen min-w-screen items-center justify-center bg-gray-500 text-white">
        <div className="border-pink-fp flex flex-col items-center justify-center gap-8 rounded-sm border-3 p-10 shadow-[0px_0px_11px_2px_#B7B2B240]">
          <img src="/img/fp-logo-alt.svg" className="w-48" />
          <ButtonPrimary
            className="shadow-[0px_0px_11px_2px_#B7B2B240]"
            onPress={connect}
          >
            Connect Device
          </ButtonPrimary>
        </div>
      </main>
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
          <a href="https://atov.de" target="_blank">
            <img src="/img/atov-logo.svg" className="w-16" />
          </a>
          <div className="flex items-center gap-4">
            <a href="https://atov.de/discord" target="_blank">
              <Icon className="h-6 w-6" name="discord" />
            </a>
            <a href="https://github.com/ATOVproject/faderpunk" target="_blank">
              <Icon className="h-6 w-6" name="github" />
            </a>
            <a href="https://www.instagram.com/atovproject/" target="_blank">
              <Icon className="h-6 w-6" name="instagram" />
            </a>
          </div>
        </div>
      </div>
    </Layout>
  );
};

export default App;
