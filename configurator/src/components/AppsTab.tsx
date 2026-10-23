import { ModalMode, type AllApps, type AppLayout } from "../utils/types";
import { useModalContext } from "../contexts/ModalContext";
import { ChannelOverview } from "./ChannelOverview";
import { GroupedApps } from "./GroupedApps";

interface Props {
  apps?: AllApps;
  layout?: AppLayout;
}

export const AppsTab = ({ apps, layout }: Props) => {
  const { setModalConfig } = useModalContext();

  return (
    <div>
      <div className="mb-12">
        <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
          Channel Overview
        </h2>
        <ChannelOverview
          onClick={() =>
            setModalConfig({ isOpen: true, mode: ModalMode.EditLayout })
          }
          layout={layout}
        />
      </div>
      {apps ? <GroupedApps apps={apps} /> : null}
    </div>
  );
};
