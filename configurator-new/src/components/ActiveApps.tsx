import type { AppLayout, App } from "../utils/types";
import { ActiveApp } from "./ActiveApp";

interface Props {
  layout?: AppLayout;
}

export const ActiveApps = ({ layout }: Props) => {
  // TODO: Skeleton loader
  if (!layout) {
    return null;
  }
  return (
    <ul className="space-y-6">
      {layout
        .filter(
          (slot): slot is { app: App; id: number; startChannel: number } =>
            !!slot.app,
        )
        .map(({ app, id, startChannel }) => {
          return (
            <li key={id}>
              <ActiveApp app={app} startChannel={startChannel} layoutId={id} />
            </li>
          );
        })}
    </ul>
  );
};
