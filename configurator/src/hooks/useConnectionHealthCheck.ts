import { useEffect, useRef } from "react";

import { useStore } from "../store";
import { getGlobalConfig } from "../utils/config";

const POLL_INTERVAL_MS = 2000;

export const useConnectionHealthCheck = () => {
  const { usbDevice, setConfig, disconnect } = useStore();
  const pollingRef = useRef(false);

  useEffect(() => {
    if (!usbDevice) return;

    const interval = setInterval(async () => {
      if (pollingRef.current) return;
      pollingRef.current = true;

      try {
        const config = await getGlobalConfig(usbDevice);
        setConfig(config);
      } catch {
        clearInterval(interval);
        disconnect();
        sessionStorage.setItem("fp-connection-lost", "1");
        window.location.href = "/";
      } finally {
        pollingRef.current = false;
      }
    }, POLL_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [usbDevice, setConfig, disconnect]);
};
