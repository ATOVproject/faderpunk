import { major, minor } from "semver";

import { APP_LIBRARY } from "../generated/appLibrary";
import { useLatestFirmwareVersion } from "../useLatestFirmwareVersion";
import { AppGrid, EmbedPage } from "./AppGrid";

function versionPath(version: string) {
  return `/${major(version)}.${minor(version)}/`;
}

export default function App() {
  const latestVersion = useLatestFirmwareVersion();

  return (
    <EmbedPage>
      <AppGrid
        apps={APP_LIBRARY.map((app) => ({
          ...app,
          href: `${versionPath(latestVersion)}#/manual#app-${app.id}`,
        }))}
      />
    </EmbedPage>
  );
}
