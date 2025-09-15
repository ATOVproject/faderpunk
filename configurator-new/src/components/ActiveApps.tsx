import type { AppInLayout, AppLayout } from "../utils/types";
import { ActiveApp } from "./ActiveApp";

interface Props {
  apps?: AppLayout;
}

// NEXT:
// - Create Select/Input components for all Value types
// - Render param inputs (use loading screen)
// - Set param values implementation
// - Somehow we need a success message. See if sticky params are a problem

export const ActiveApps = ({ apps }: Props) => {
  // TODO: Skeleton loader
  if (!apps) {
    return null;
  }
  return (
    <ul className="space-y-6">
      {apps
        .filter((app): app is AppInLayout => !("slotNumber" in app))
        .map((app) => {
          return (
            <li key={app.start}>
              <ActiveApp app={app} />
            </li>
          );
        })}
    </ul>
  );
};
