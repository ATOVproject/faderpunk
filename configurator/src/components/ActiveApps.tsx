import { useStore } from "../store";
import type { App } from "../utils/types";
import { ActiveApp } from "./ActiveApp";

export const ActiveApps = () => {
  const { params: allParams, layout } = useStore();
  if (!layout || !layout.some((slot) => !!slot.app) || !allParams) {
    return null;
  }
  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Active Apps
      </h2>
      <ul className="space-y-6">
        {layout
          .filter(
            (
              slot,
            ): slot is {
              app: App;
              id: number;
              startChannel: number;
            } => !!slot.app,
          )
          .map(({ app, id, startChannel }) => {
            const params = allParams.get(id);
            if (!params) {
              return null;
            }
            return (
              <li key={id}>
                <ActiveApp
                  app={app}
                  startChannel={startChannel}
                  layoutId={id}
                  params={params}
                />
              </li>
            );
          })}
      </ul>
    </div>
  );
};
