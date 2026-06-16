import { useEffect, useRef } from "react";
import { addToast } from "@heroui/toast";

import { useStore } from "../store";
import { getGlobalConfig } from "../utils/config";

const POLL_INTERVAL_MS = 2000;

export const useConnectionHealthCheck = () => {
  const { usbDevice, isSimulator, setConfig, disconnect } = useStore();
  const pollingRef = useRef(false);

  useEffect(() => {
    if (!usbDevice || isSimulator) return;

    const interval = setInterval(async () => {
      if (pollingRef.current) return;
      pollingRef.current = true;

      try {
        const config = await getGlobalConfig(usbDevice);
        setConfig(config);
      } catch {
        clearInterval(interval);
        // Drop back into the simulator in place rather than reloading to a
        // landing page (there no longer is one), and tell the user why.
        disconnect();
        addToast({
          title: "Device disconnected",
          description:
            "I don't see the Faderpunk anymore — check your cabling and connect again.",
          color: "warning",
        });
      } finally {
        pollingRef.current = false;
      }
    }, POLL_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [usbDevice, isSimulator, setConfig, disconnect]);
};
