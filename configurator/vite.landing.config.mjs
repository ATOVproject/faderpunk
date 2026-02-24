import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";
import { getFirmwareVersion } from "./vite.firmware-version.mjs";

// https://vite.dev/config/
export default defineConfig({
  base: "/",
  root: "src/landing",
  publicDir: "../../public",
  plugins: [react(), tailwindcss()],
  define: {
    __FIRMWARE_LATEST_VERSION__: JSON.stringify(getFirmwareVersion()),
  },
  build: {
    outDir: "../../dist-landing",
    emptyOutDir: true,
  },
});
