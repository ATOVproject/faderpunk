import { useEffect, useState } from "react";

import {
  FPAPP_MANUALS_UPDATED_EVENT,
  getCachedFpAppManuals,
} from "../utils/fpapp";

// Manuals cached when a community app is installed, kept in sync with
// installs/removals here and in other browser tabs.
export const useCachedFpAppManuals = () => {
  const [manuals, setManuals] = useState(getCachedFpAppManuals);
  useEffect(() => {
    const refresh = () => setManuals(getCachedFpAppManuals());
    window.addEventListener(FPAPP_MANUALS_UPDATED_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(FPAPP_MANUALS_UPDATED_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);
  return manuals;
};
