import type { ManualAppData } from "../components/manual/ManualApp";
import { parseFpAppManual, type CachedFpAppManual } from "../utils/fpapp";
import { useCachedFpAppManuals } from "./useCachedFpAppManuals";

function toManualApp(manual: CachedFpAppManual): ManualAppData | undefined {
  if (!manual.manual) return undefined;
  try {
    const app = parseFpAppManual(manual.manual, manual.appId);
    return app
      ? {
          ...app,
          setup: manual.setup ? withoutHeading(manual.setup) : undefined,
          version: manual.version,
        }
      : undefined;
  } catch {
    return undefined;
  }
}

export function withoutHeading(markdown: string, manual?: CachedFpAppManual) {
  const lines = markdown.split("\n");
  if (lines[0]?.startsWith("# ")) lines.shift();
  while (lines[0]?.trim() === "") lines.shift();
  if (manual && lines[0]?.trim() === manual.description.trim()) lines.shift();
  while (lines[0]?.trim() === "") lines.shift();
  return lines.join("\n");
}

export interface InstalledManualView {
  cached: CachedFpAppManual;
  app?: ManualAppData;
}

// Manuals of the community apps currently installed on the device, cached
// locally when each app is installed (see InstalledApps).
export const useInstalledManualViews = (): InstalledManualView[] => {
  const installedManuals = useCachedFpAppManuals();
  return installedManuals.map((cached) => ({
    cached,
    app: toManualApp(cached),
  }));
};
