import type { AppLayout } from "../utils/types";
import { ActiveApps } from "./ActiveApps";
import { ChannelOverview } from "./ChannelOverview";

interface Props {
  layout?: AppLayout;
  setModalApp(app: number | null): void;
}

export const DeviceTab = ({ layout, setModalApp }: Props) => (
  <div>
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Channel Overview
      </h2>
      <ChannelOverview onClick={() => setModalApp(-1)} layout={layout} />
    </div>
    <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
      Active Apps
    </h2>
    <ActiveApps layout={layout} />
  </div>
);
