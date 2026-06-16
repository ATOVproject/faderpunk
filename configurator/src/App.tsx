import { useEffect, useState } from "react";
import { Routes, Route, Navigate, useLocation } from "react-router-dom";

import { useStore } from "./store";
import { useConnectionHealthCheck } from "./hooks/useConnectionHealthCheck";
import { ConfiguratorPage } from "./components/ConfiguratorPage";
import { AboutPage } from "./components/AboutPage";
import { ManualPage } from "./components/ManualPage";
import { UpdatePage } from "./components/UpdatePage";
import { TroubleshootingPage } from "./components/TroubleshootingPage";

const DEVICELESS_ROUTES = ["/about", "/manual", "/update", "/troubleshooting"];

const App = () => {
  const { usbDevice, isSimulator, autoConnect, connectSimulator } = useStore();
  const location = useLocation();
  useConnectionHealthCheck();
  const skipAutoConnect =
    DEVICELESS_ROUTES.includes(location.pathname) ||
    sessionStorage.getItem("fp-skip-autoconnect") === "1";
  const [isAutoConnecting, setIsAutoConnecting] = useState(!skipAutoConnect);

  useEffect(() => {
    const boot = async () => {
      // Deviceless info pages (and the post-update redirect) must not grab the
      // USB device. Drop straight into the simulator so the app always has
      // working state and never lands on a blank screen.
      if (skipAutoConnect) {
        sessionStorage.removeItem("fp-skip-autoconnect");
        connectSimulator();
        setIsAutoConnecting(false);
        return;
      }

      // Reconnect to an already-paired device if there is one; otherwise the
      // simulator is the default experience.
      if (!usbDevice) {
        const connected = await autoConnect();
        if (!connected) connectSimulator();
      }
      setIsAutoConnecting(false);
    };
    boot();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (isAutoConnecting) {
    return null;
  }

  return (
    <Routes>
      <Route path="/" element={<Navigate to="/configurator" replace />} />
      <Route
        path="/configurator"
        element={
          usbDevice || isSimulator ? (
            <ConfiguratorPage />
          ) : (
            <Navigate to="/" replace />
          )
        }
      />
      <Route path="/about" element={<AboutPage />} />
      <Route path="/manual" element={<ManualPage />} />
      <Route path="/update" element={<UpdatePage />} />
      <Route path="/troubleshooting" element={<TroubleshootingPage />} />
    </Routes>
  );
};

export default App;
