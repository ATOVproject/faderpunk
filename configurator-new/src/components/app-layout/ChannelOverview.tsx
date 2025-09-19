import classNames from "classnames";

import { COLORS_CLASSES, WIDTHS_CLASSES } from "../../utils/class-helpers";
import type { AppLayout, App } from "../../utils/types";
import { getSlots } from "../../utils/utils";

interface AppSlotProps {
  app: App;
  startChannel: number;
}

const AppSlots = ({ app, startChannel }: AppSlotProps) => {
  return (
    <div
      className={classNames(
        "flex flex-col gap-2 rounded-md bg-black p-2",
        WIDTHS_CLASSES[Number(app.channels)],
      )}
    >
      <div
        className={classNames("flex-1 rounded-md", COLORS_CLASSES[app.color])}
      >
        &nbsp;
      </div>
      <div className="flex-1 text-center text-base font-bold">
        {getSlots(app, startChannel)}
      </div>
    </div>
  );
};

interface EmptySlotProps {
  slotNumber: number;
}

const EmptySlot = ({ slotNumber }: EmptySlotProps) => (
  <div className="flex grow-1 flex-col gap-2 rounded-md bg-black p-2">
    <div className="transparent flex-1 rounded-md">&nbsp;</div>
    <div className="flex-1 text-center text-base font-bold">
      {slotNumber + 1}
    </div>
  </div>
);

interface Props {
  layout?: AppLayout;
  onClick(): void;
}

export const ChannelOverview = ({ layout, onClick }: Props) => {
  // TODO: Loading skeleton
  if (!layout) {
    return null;
  }

  return (
    <button
      className="grid w-full cursor-pointer grid-cols-16 gap-2"
      onClick={onClick}
    >
      {layout.map(({ id, app, startChannel }) => {
        if (app) {
          return <AppSlots startChannel={startChannel} key={id} app={app} />;
        }
        return <EmptySlot key={id} slotNumber={startChannel} />;
      })}
    </button>
  );
};
