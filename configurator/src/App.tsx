import { useEffect, useState } from "react";
import { Routes, Route, Navigate, useLocation } from "react-router-dom";

import { useStore } from "./store";
import { IS_SIMULATOR_BUILD } from "./consts";
import { useConnectionHealthCheck } from "./hooks/useConnectionHealthCheck";
import { ConfiguratorPage } from "./components/ConfiguratorPage";
import { AboutPage } from "./components/AboutPage";
import { ConnectPage } from "./components/ConnectPage";
import { ManualPage } from "./components/ManualPage";
import { UpdatePage } from "./components/UpdatePage";
import { TroubleshootingPage } from "./components/TroubleshootingPage";

const DEVICELESS_ROUTES = ["/about", "/manual", "/update", "/troubleshooting"];

const App = () => {
  const { device, isSimulator, autoConnect, connectSimulator } = useStore();
  const location = useLocation();
  useConnectionHealthCheck();
  const skipAutoConnect =
    DEVICELESS_ROUTES.includes(location.pathname) ||
    sessionStorage.getItem("fp-skip-autoconnect") === "1";
  const [isAutoConnecting, setIsAutoConnecting] = useState(!skipAutoConnect);

  useEffect(() => {
    // The dedicated simulator build skips the connect page entirely.
    if (IS_SIMULATOR_BUILD) {
      connectSimulator();
      setIsAutoConnecting(false);
      return;
    }
    if (skipAutoConnect) {
      sessionStorage.removeItem("fp-skip-autoconnect");
      return;
    }
    const attemptAutoConnect = async () => {
      if (!device) {
        await autoConnect();
      }
      setIsAutoConnecting(false);
    };
    attemptAutoConnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (isAutoConnecting) {
    return null;
  }

  return (
    <Routes>
      <Route
        path="/"
        element={
          device || isSimulator ? (
            <Navigate to="/configurator" replace />
          ) : (
            <ConnectPage />
          )
        }
      />
      <Route
        path="/configurator"
        element={
          device || isSimulator ? (
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
