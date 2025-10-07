import { Routes, Route, Navigate, useNavigate } from "react-router-dom";

import { useStore } from "./store";
import { ConfiguratorPage } from "./components/ConfiguratorPage";
import { AboutPage } from "./components/AboutPage";
import { ConnectPage } from "./components/ConnectPage";
import { ManualPage } from "./components/ManualPage";
import { UpdatePage } from "./components/UpdatePage";
import { useEffect } from "react";

const App = () => {
  const { usbDevice, deviceVersion } = useStore();
  const navigate = useNavigate();

  useEffect(() => {
    if (deviceVersion && deviceVersion !== "1.3.0") {
      navigate("/update");
    }
  }, [deviceVersion, navigate]);

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
    </Routes>
  );
};

export default App;
