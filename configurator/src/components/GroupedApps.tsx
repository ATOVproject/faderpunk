import { useMemo } from "react";
import type { AllApps } from "../utils/types";
import { groupAndSortApps } from "../utils/utils";
import { AppSection } from "./AppSection";

interface Props {
  apps: AllApps;
  setModalApp(app: number | null): void;
}

export const GroupedApps = ({ apps, setModalApp }: Props) => {
  const groupedApps = useMemo(() => groupAndSortApps(apps), [apps]);
  return (
    <div>
      {groupedApps.map((section) =>
        section.length ? (
          <AppSection
            key={section[0].channels}
            section={section}
            channels={Number(section[0].channels)}
            setModalApp={setModalApp}
          />
        ) : null,
      )}
    </div>
  );
};
