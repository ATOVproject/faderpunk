import type { AppLayout, App } from "../utils/types";
import { ActiveApp } from "./ActiveApp";

interface Props {
  layout?: AppLayout;
}

export const ActiveApps = ({ layout }: Props) => {
  // TODO: Skeleton loader
  if (!layout || !layout.length || layout.every(({ app }) => !app)) {
    return null;
  }
  return (
    <>
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Active Apps
      </h2>
      <ul className="space-y-6">
        {layout
          .filter(
            (slot): slot is { app: App; id: number; startChannel: number } =>
              !!slot.app,
          )
          .map(({ app, id, startChannel }) => {
            return (
              <li key={id}>
                <ActiveApp
                  app={app}
                  startChannel={startChannel}
                  layoutId={id}
                />
              </li>
            );
          })}
      </ul>
    </>
  );
};
