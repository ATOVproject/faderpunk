import { type ManualAppData, ManualApp } from "./ManualApp";
import { H2 } from "./Shared";

interface Props {
  apps: ManualAppData[];
}

export const InstalledCommunityApps = ({ apps }: Props) =>
  apps.length > 0 ? (
    <>
      <H2 id="installed-community-apps">
        Installed Community Apps — Unofficial, Unmaintained
      </H2>
      {apps.map((app) => (
        <ManualApp key={app.appId} app={app} />
      ))}
    </>
  ) : null;
