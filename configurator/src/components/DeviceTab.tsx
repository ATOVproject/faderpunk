import { ModalMode, type AppLayout } from "../utils/types";
import { useModalContext } from "../contexts/ModalContext";
import { ActiveApps } from "./ActiveApps";
import { ChannelOverview } from "./ChannelOverview";
import { SaveLoadLayout } from "./SaveLoadLayout";

interface Props {
  layout?: AppLayout;
}

export const DeviceTab = ({ layout }: Props) => {
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
      <ActiveApps />
      <SaveLoadLayout />
    </div>
  );
};
