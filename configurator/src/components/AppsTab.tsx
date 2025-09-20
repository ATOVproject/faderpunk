import type { AllApps } from "../utils/types";
import { GroupedApps } from "./GroupedApps";

interface Props {
  apps?: AllApps;
  setModalApp(app: number | null): void;
}

export const AppsTab = ({ apps, setModalApp }: Props) => {
  if (!apps) return null;
  return <GroupedApps setModalApp={setModalApp} apps={apps} />;
};
