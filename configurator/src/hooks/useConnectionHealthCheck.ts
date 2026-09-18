import { useEffect, useRef } from "react";

import { useStore } from "../store";
import { getChangedAppParams, getGlobalConfig } from "../utils/config";

const POLL_INTERVAL_MS = 2000;

export const useConnectionHealthCheck = () => {
  const { device, isSimulator, setConfig, setParams, disconnect } = useStore();
  const pollingRef = useRef(false);

  useEffect(() => {
    if (!device || isSimulator) return;

    const interval = setInterval(async () => {
      if (
        pollingRef.current ||
        useStore.getState().suspendHealthCheck ||
        useStore.getState().rebooting
      )
        return;
      pollingRef.current = true;

      try {
        const config = await getGlobalConfig(device);
        setConfig(config);
      } catch {
        clearInterval(interval);
        disconnect();
        sessionStorage.setItem("fp-connection-lost", "1");
        window.location.href = "/";
        return;
      } finally {
        pollingRef.current = false;
      }

      // Best-effort: an app can change its own params on the device (a panel
      // gesture, e.g. Manifold's Mode cycle) with no way to reach the
      // Configurator on its own — see the #640 review for why the device
      // doesn't push this instead. Piggybacks on the same interval as the
      // check above rather than a second poller, but is never allowed to
      // affect connection health: an old firmware that predates this message,
      // or one transient failure, should not read as a dropped connection.
      try {
        const changed = await getChangedAppParams(device);
        for (const [layoutId, values] of changed) {
          setParams(layoutId, values);
        }
      } catch {
        // Ignored — see above.
      }
    }, POLL_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [device, isSimulator, setConfig, setParams, disconnect]);
};
