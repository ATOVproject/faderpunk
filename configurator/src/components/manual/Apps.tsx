import { type ManualAppData, ManualApp } from "./ManualApp";
import { H2 } from "./Shared";

interface Props {
  apps: ManualAppData[];
}

export const Apps = ({ apps }: Props) => (
  <>
    <H2 id="apps">Apps</H2>
    {apps.map((app) => (
      <ManualApp key={app.appId} app={app} />
    ))}
  </>
);
