import type { ReactNode } from "react";
import classNames from "classnames";

import { ModalMode, type App } from "../utils/types";
import { useModalContext } from "../contexts/ModalContext";
import { Icon } from "./Icon";
import { pascalToKebab } from "../utils/utils";
import { COLORS_CLASSES } from "../utils/class-helpers";

interface Props {
  channels: number;
  section: App[];
  // Extra control shown under each card. Kept outside the card's button,
  // since a button can't contain another interactive element.
  renderAction?: (app: App) => ReactNode;
}

export const AppSection = ({ channels, section, renderAction }: Props) => {
  const { setModalConfig } = useModalContext();

  return (
    <div className="mb-15 last:mb-0">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        {channels} {channels > 1 ? "channels" : "channel"}
      </h2>
      <div className="grid grid-cols-4 gap-x-6 gap-y-10">
        {section.map((app) => (
          <div key={app.appId}>
            <button
              className="flex w-full cursor-pointer gap-x-4 rounded-sm bg-black"
              onClick={() =>
                setModalConfig({
                  isOpen: true,
                  appToAdd: app.appId,
                  mode: ModalMode.AddApp,
                })
              }
            >
              <div
                className={classNames(
                  "rounded-sm p-2",
                  COLORS_CLASSES[app.color].bg,
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
            {renderAction?.(app)}
          </div>
        ))}
      </div>
    </div>
  );
};
