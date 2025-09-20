import type { AppLayout } from "../utils/types";
import { ActiveApps } from "./ActiveApps";

interface Props {
  layout?: AppLayout;
}

export const DeviceTab = ({ layout }: Props) => (
  <div>
    <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
      Active Apps
    </h2>
    <ActiveApps layout={layout} />
  </div>
);
