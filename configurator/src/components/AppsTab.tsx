import type { AllApps, AppLayout } from "../utils/types";
import { ChannelOverview } from "./ChannelOverview";
import { GroupedApps } from "./GroupedApps";

interface Props {
  apps?: AllApps;
  layout?: AppLayout;
  setModalApp(app: number | null): void;
}

export const AppsTab = ({ apps, layout, setModalApp }: Props) => (
  <div>
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Channel Overview
      </h2>
      <ChannelOverview onClick={() => setModalApp(-1)} layout={layout} />
    </div>
    {apps ? <GroupedApps setModalApp={setModalApp} apps={apps} /> : null}
  </div>
);
