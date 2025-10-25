import { Routes, Route, Navigate } from "react-router-dom";

import { useStore } from "./store";
import { ConfiguratorPage } from "./components/ConfiguratorPage";
import { AboutPage } from "./components/AboutPage";
import { ConnectPage } from "./components/ConnectPage";
import { ManualPage } from "./components/ManualPage";
import { UpdatePage } from "./components/UpdatePage";
import { TroubleshootingPage } from "./components/TroubleshootingPage";

const App = () => {
  const { usbDevice } = useStore();

  return (
    <Routes>
      <Route
        path="/"
        element={
          usbDevice ? <Navigate to="/configurator" replace /> : <ConnectPage />
        }
      />
      <Route
        path="/configurator"
        element={usbDevice ? <ConfiguratorPage /> : <Navigate to="/" replace />}
      />
      <Route path="/about" element={<AboutPage />} />
      <Route path="/manual" element={<ManualPage />} />
      <Route path="/update" element={<UpdatePage />} />
      <Route path="/troubleshooting" element={<TroubleshootingPage />} />
    </Routes>
  );
};

export default App;
