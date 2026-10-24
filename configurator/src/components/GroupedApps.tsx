import { useMemo } from "react";
import type { AllApps, App } from "../utils/types";
import { groupAndSortApps } from "../utils/utils";
import { AppSection } from "./AppSection";
import { useCachedFpAppManuals } from "../hooks/useCachedFpAppManuals";
import { useManualNav } from "../contexts/ManualNavContext";

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

const CATALOGUE_URL = "https://atovproject.github.io/faderpunk-community-apps/";

// Under each community app's card: its manual in the Manual tab when it was
// cached on this browser at install, else its entry in the online catalogue
// (e.g. an app installed from another computer).
const ManualLink = ({ app }: { app: App }) => {
  const manuals = useCachedFpAppManuals();
  const { showInManual } = useManualNav();
  const className =
    "mt-2 inline-block cursor-pointer text-sm text-gray-300 underline";
  return manuals.some((manual) => manual.appId === app.appId) ? (
    <button
      className={className}
      onClick={() => showInManual(`app-${app.appId}`)}
    >
      Read manual
    </button>
  ) : (
    <a
      className={className}
      href={`${CATALOGUE_URL}#app-${app.appId}`}
      target="_blank"
      rel="noopener noreferrer"
    >
      Find manual in catalogue
    </a>
  );
};

const AppSections = ({
  apps,
  withManualLink,
}: {
  apps: App[];
  withManualLink?: boolean;
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
            renderAction={
              withManualLink ? (app) => <ManualLink app={app} /> : undefined
            }
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
    <div className="mb-12">
      <AppSections apps={official} />
      {community.length > 0 ? (
        <div>
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Community Apps — Unofficial, Unmaintained
          </h2>
          <AppSections apps={community} withManualLink />
        </div>
      ) : null}
    </div>
  );
};
