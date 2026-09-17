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
            // A missing entry is normal for an app with no params (it never
            // gets asked). For one that declares params, it means the app
            // never answered a param request — still worth a card, so the
            // silence is visible instead of the app just not being listed.
            const notResponding = app.paramCount > 0 && !params;
            return (
              <li key={id}>
                <ActiveApp
                  app={app}
                  startChannel={startChannel}
                  layoutId={id}
                  params={params ?? []}
                  notResponding={notResponding}
                />
              </li>
            );
          })}
      </ul>
    </div>
  );
};
