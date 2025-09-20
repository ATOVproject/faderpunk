import classNames from "classnames";

import type { App } from "../utils/types";
import { Icon } from "./Icon";
import { pascalToKebab } from "../utils/utils";
import { COLORS_CLASSES } from "../utils/class-helpers";

interface Props {
  channels: number;
  section: App[];
  setModalApp(app: number | null): void;
}

export const AppSection = ({ channels, section, setModalApp }: Props) => {
  return (
    <div className="mb-15">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        {channels} {channels > 1 ? "channels" : "channel"}
      </h2>
      <div className="grid grid-cols-4 gap-x-6 gap-y-10">
        {section.map((app) => (
          <button
            className="flex cursor-pointer gap-x-4 rounded-sm bg-black"
            key={app.appId}
            onClick={() => setModalApp(app.appId)}
          >
            <div
              className={classNames(
                "rounded-sm p-2",
                COLORS_CLASSES[app.color],
              )}
            >
              <Icon
                className="h-12 w-12 text-black"
                name={pascalToKebab(app.icon)}
              />
            </div>
            <span className="text-md flex items-center font-bold">
              {app.name}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
};
