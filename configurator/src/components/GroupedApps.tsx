import { useMemo } from "react";
import type { AllApps } from "../utils/types";
import { groupAndSortApps } from "../utils/utils";
import { AppSection } from "./AppSection";

interface Props {
  apps: AllApps;
}

export const GroupedApps = ({ apps }: Props) => {
  const groupedApps = useMemo(() => groupAndSortApps(apps), [apps]);
  return (
    <div>
      {groupedApps.map((section) =>
        section.length ? (
          <AppSection
            key={section[0].channels}
            section={section}
            channels={Number(section[0].channels)}
          />
        ) : null,
      )}
    </div>
  );
};
