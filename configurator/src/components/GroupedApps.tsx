import { useMemo, type ReactNode } from "react";
import type { AllApps, App } from "../utils/types";
import { groupAndSortApps } from "../utils/utils";
import { AppSection } from "./AppSection";

// Community app IDs start at 100 (official apps use 1-99) -- see
// faderpunk-community-apps' README. These only show up in the Community tab
// when the connected device actually reports appId >= 100 entries, which
// today means it has an FPApp installed: the firmware appends one AppConfig
// per occupied slot to its GetAllApps response, alongside the built-ins. A
// device with no community app installed reports none, so CommunityApps
// below simply renders nothing for it.
const FIRST_COMMUNITY_APP_ID = 100;

interface Props {
  apps: AllApps;
  renderAction?: (app: App) => ReactNode;
}

const AppSections = ({
  apps,
  renderAction,
}: {
  apps: App[];
  renderAction?: (app: App) => ReactNode;
}) => {
  const groupedApps = useMemo(
    () => groupAndSortApps(new Map(apps.map((app) => [app.appId, app]))),
    [apps],
  );
  return (
    <>
      {groupedApps.map((section) =>
        section.length ? (
          <AppSection
            key={section[0].channels}
            section={section}
            channels={Number(section[0].channels)}
            renderAction={renderAction}
          />
        ) : null,
      )}
    </>
  );
};

const isCommunity = (app: App) => app.appId >= FIRST_COMMUNITY_APP_ID;

export const GroupedApps = ({ apps }: Props) => {
  const official = useMemo(
    () => [...apps.values()].filter((app) => !isCommunity(app)),
    [apps],
  );

  return (
    <div className="mb-12">
      <AppSections apps={official} />
    </div>
  );
};

export const CommunityApps = ({ apps, renderAction }: Props) => {
  const community = useMemo(
    () => [...apps.values()].filter(isCommunity),
    [apps],
  );

  return community.length > 0 ? (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Installed Community Apps
      </h2>
      <AppSections apps={community} renderAction={renderAction} />
    </div>
  ) : null;
};
