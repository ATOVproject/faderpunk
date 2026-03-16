import { useEffect, useState } from "react";

import { FIRMWARE_LATEST_VERSION } from "./consts";

export function useLatestFirmwareVersion() {
  const [latestVersion, setLatestVersion] = useState(FIRMWARE_LATEST_VERSION);

  useEffect(() => {
    fetch("/version.json")
      .then((r) => (r.ok ? r.json() : Promise.reject()))
      .then((data: { firmware: string }) => {
        if (data.firmware) setLatestVersion(data.firmware);
      })
      .catch(() => {});
  }, []);

  return latestVersion;
}
