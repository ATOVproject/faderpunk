import classNames from "classnames";
import { COLORS_CLASSES, WIDTHS_CLASSES } from "../class-helpers";
import type { App } from "../types";

interface Props {
  activeApps: App[];
}

export const ChannelOverview = ({ activeApps }: Props) => {
  return (
    <div className="flex gap-2">
      {activeApps.map((app) => (
        <div
          className={classNames(
            "flex flex-col gap-2 rounded-md bg-black p-2",
            WIDTHS_CLASSES[app.channels],
          )}
        >
          <div
            className={classNames(
              "flex-1 rounded-md",
              COLORS_CLASSES[app.color || "Transparent"],
            )}
          >
            &nbsp;
          </div>
          <div className="flex-1 text-center text-base font-bold">
            {app.slots}
          </div>
        </div>
      ))}
    </div>
  );
};
