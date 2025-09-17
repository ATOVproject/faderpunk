import { useDisclosure } from "@heroui/react";

import { ButtonPrimary } from "./components/Button";
import { ChannelOverview } from "./components/app-layout/ChannelOverview";
import { Icon } from "./components/Icon";
import { Layout } from "./components/Layout";
import { ActiveApps } from "./components/ActiveApps";
import { useStore } from "./store";

const App = () => {
  const { usbDevice, layout, connect } = useStore();
  const { isOpen, onOpen, onOpenChange } = useDisclosure();

  // TODO: Why is the layout scrolling??
  if (!usbDevice) {
    return (
      <Layout isModalOpen={isOpen} onModalOpenChange={onOpenChange}>
        <ButtonPrimary onPress={connect}>Connect</ButtonPrimary>
      </Layout>
    );
  }

  return (
    <Layout isModalOpen={isOpen} onModalOpenChange={onOpenChange}>
      <div className="mb-8 text-center">
        <img src="/img/fp-logo.svg" className="inline w-64" />
        <p className="font-vox mt-3 text-xl font-semibold tracking-wider text-white uppercase">
          Configurator
        </p>
      </div>

      <div className="border-default-100 mb-8 flex border-b-3">
        <div className="rounded-t-md bg-black px-12 py-3 text-lg font-bold text-white uppercase">
          Device
        </div>
        <div className="rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase">
          Apps
        </div>
        <div className="rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase">
          Settings
        </div>
      </div>

      <div className="mb-12">
        <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
          Channel Overview
        </h2>
        <ChannelOverview onClick={onOpen} apps={layout} />
      </div>

      <div>
        <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
          Active Apps
        </h2>
        <ActiveApps apps={layout} />
      </div>
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
