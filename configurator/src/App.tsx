import { useState } from "react";
import { Tabs, Tab } from "@heroui/tabs";

import { ButtonPrimary } from "./components/Button";
import { Layout } from "./components/Layout";
import { useStore } from "./store";
import { DeviceTab } from "./components/DeviceTab";
import { AppsTab } from "./components/AppsTab";
import { SettingsTab } from "./components/SettingsTab";
import { About } from "./components/About";
import { Footer } from "./components/Footer";

const enum Page {
  Configurator = "Configurator",
  About = "About",
}

const App = () => {
  const { apps, config, layout, usbDevice, connect } = useStore();
  const [modalApp, setModalApp] = useState<number | null>(null);
  const [page, setPage] = useState<Page>(Page.Configurator);

  if (!usbDevice) {
    return (
      <main className="flex min-h-screen min-w-screen items-center justify-center bg-gray-500">
        {page === Page.Configurator ? (
          <div className="flex flex-col justify-center">
            <div className="border-pink-fp flex flex-col items-center justify-center gap-8 rounded-sm border-3 p-10 shadow-[0px_0px_11px_2px_#B7B2B240]">
              <img src="/img/fp-logo-alt.svg" className="w-48" />
              <ButtonPrimary
                className="shadow-[0px_0px_11px_2px_#B7B2B240]"
                onPress={connect}
              >
                Connect Device
              </ButtonPrimary>
            </div>
            <button
              className="text-default-400 mt-4 cursor-pointer underline"
              onClick={() => setPage(Page.About)}
            >
              What is this?
            </button>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center">
            <About />
            <button
              className="text-default-400 mt-4 cursor-pointer underline"
              onClick={() => setPage(Page.Configurator)}
            >
              Take me back
            </button>
          </div>
        )}
      </main>
    );
  }

  return (
    <Layout
      modalApp={modalApp}
      onModalOpenChange={(isOpen) => setModalApp(isOpen ? -1 : null)}
    >
      <div className="flex-grow">
        <div className="mb-8 text-center">
          <img src="/img/fp-logo.svg" className="inline w-64" />
          <h1 className="font-vox mt-3 text-xl font-semibold tracking-wider uppercase">
            {page}
          </h1>
        </div>
        {page === Page.Configurator ? (
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
            <Tab key="about" title="About">
              <About />
            </Tab>
          </Tabs>
        ) : (
          <About />
        )}
      </div>
      <Footer />
    </Layout>
  );
};

export default App;
