import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";

import { useStore } from "./store";
import { ConfiguratorPage } from "./components/ConfiguratorPage";
import { AboutPage } from "./components/AboutPage";
import { ConnectPage } from "./components/ConnectPage";
import { ManualPage } from "./components/ManualPage";

const App = () => {
  const { usbDevice } = useStore();

  return (
    <BrowserRouter>
      <Routes>
        <Route
          path="/"
          element={
            usbDevice ? (
              <Navigate to="/configurator" replace />
            ) : (
              <ConnectPage />
            )
          }
        />
        <Route
          path="/configurator"
          element={
            usbDevice ? <ConfiguratorPage /> : <Navigate to="/" replace />
          }
        />
        <Route path="/about" element={<AboutPage />} />
        <Route path="/manual" element={<ManualPage />} />
      </Routes>
    </BrowserRouter>
  );
};

export default App;
