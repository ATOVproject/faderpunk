import { useMemo } from "react";
import type { AllApps, App } from "../utils/types";
import { groupAndSortApps } from "../utils/utils";
import { AppSection } from "./AppSection";

// Community app IDs start at 100 (official apps use 1-99) -- see
// faderpunk-community-apps' README. These only show up here when the
// connected device actually reports appId >= 100 entries, which today
// means it has an FPApp installed: the firmware appends one AppConfig per
// occupied slot to its GetAllApps response, alongside the built-ins. A
// device with no community app installed reports none, so the "Community
// Apps" heading below simply doesn't render for it.
const FIRST_COMMUNITY_APP_ID = 100;

interface Props {
  apps: AllApps;
}

const AppSections = ({ apps }: { apps: App[] }) => {
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
          />
        ) : null,
      )}
    </>
  );
};

export const GroupedApps = ({ apps }: Props) => {
  const { official, community } = useMemo(() => {
    const official: App[] = [];
    const community: App[] = [];
    for (const app of apps.values()) {
      (app.appId >= FIRST_COMMUNITY_APP_ID ? community : official).push(app);
    }
    return { official, community };
  }, [apps]);

  return (
    <div>
      <AppSections apps={official} />
      {community.length > 0 ? (
        <div>
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Community Apps — Unofficial, Unmaintained
          </h2>
          <AppSections apps={community} />
        </div>
      ) : null}
    </div>
  );
};
