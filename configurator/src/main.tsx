import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { HeroUIProvider } from "@heroui/system";
import { ToastProvider } from "@heroui/toast";
import "./index.css";
import App from "./App.tsx";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <HeroUIProvider>
      <ToastProvider placement="top-center" />
      <HashRouter>
        <App />
      </HashRouter>
    </HeroUIProvider>
  </StrictMode>,
);
