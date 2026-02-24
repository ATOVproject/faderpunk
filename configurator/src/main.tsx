import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { HeroUIProvider } from "@heroui/system";

import "./index.css";
import App from "./App.tsx";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <HeroUIProvider>
      <HashRouter>
        <App />
      </HashRouter>
    </HeroUIProvider>
  </StrictMode>,
);
