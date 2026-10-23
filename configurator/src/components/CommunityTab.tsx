import type { PropsWithChildren } from "react";

import type { AllApps, App } from "../utils/types";
import { useCachedFpAppManuals } from "../hooks/useCachedFpAppManuals";
import { useManualNav } from "../contexts/ManualNavContext";
import { CommunityApps } from "./GroupedApps";
import { InstalledApps } from "./FpAppsTab";
import { CommunityCatalogue } from "./manual/CommunityCatalogue";
import { CommunityTools } from "./manual/CommunityTools";

const COMMUNITY_REPO =
  "https://github.com/ATOVproject/faderpunk-community-apps";
const FIRMWARE_REPO = "https://github.com/ATOVproject/faderpunk";

interface Props {
  apps?: AllApps;
}

const ExternalLink = ({
  href,
  children,
}: PropsWithChildren<{ href: string }>) => (
  <a
    className="font-semibold underline"
    href={href}
    target="_blank"
    rel="noopener noreferrer"
  >
    {children}
  </a>
);

const Card = ({ title, children }: PropsWithChildren<{ title: string }>) => (
  <div className="rounded-sm bg-black p-6">
    <h3 className="text-yellow-fp mb-3 text-sm font-bold uppercase">{title}</h3>
    <div className="space-y-3 text-sm leading-6">{children}</div>
  </div>
);

const scrollToId = (id: string) =>
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth" });

export const CommunityTab = ({ apps }: Props) => {
  const manuals = useCachedFpAppManuals();
  const { showInManual } = useManualNav();
  // An app's manual is cached on this browser when it is installed here, and
  // then lives in the Manual tab; for one that isn't (installed from another
  // computer), the catalogue below has it.
  const renderManualLink = (app: App) => {
    const cached = manuals.some((manual) => manual.appId === app.appId);
    return (
      <button
        className="mt-2 cursor-pointer text-sm text-gray-300 underline"
        onClick={() =>
          cached
            ? showInManual(`app-${app.appId}`)
            : scrollToId("community-catalogue")
        }
      >
        {cached ? "Read manual" : "Find manual in catalogue"}
      </button>
    );
  };

  return (
    <div>
      <div className="mb-12 max-w-3xl">
        <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
          Community Apps — Unofficial, Unmaintained
        </h2>
        <p className="mb-3">
          Community apps are built and shared by Faderpunk players and
          developers. They install into a free slot without reflashing firmware,
          and sit next to the official apps in your layout.
        </p>
        <p>
          They are not made, reviewed or supported by ATOV, and come without
          guarantees: install only apps you trust. If one ever stops the device
          responding, a factory reset removes it (see the Troubleshooting
          section of the Manual).
        </p>
      </div>

      <div className="mb-12 grid grid-cols-1 gap-6 md:grid-cols-2">
        <Card title="Submit an app">
          <p>
            Submissions are pull requests to the{" "}
            <ExternalLink href={COMMUNITY_REPO}>
              faderpunk-community-apps
            </ExternalLink>{" "}
            repository. One pull request adds one app:
          </p>
          <ul className="ml-4 list-disc">
            <li>
              <code>apps/&lt;name&gt;.rs</code> — the app, written like an
              official one
            </li>
            <li>
              one entry in <code>manual-tab.json</code> — its manual
            </li>
            <li>
              one entry in <code>apps-catalog.json</code> — its catalogue
              listing
            </li>
          </ul>
          <p>
            Automated checks run on every pull request, and a maintainer makes
            the final merge decision. Read the{" "}
            <ExternalLink href={`${COMMUNITY_REPO}/blob/main/CONTRIBUTING.md`}>
              contribution guide
            </ExternalLink>{" "}
            for the rules and the other kinds of pull request (fixes, manual
            edits).
          </p>
        </Card>
        <Card title="Feedback & bug reports">
          <ul className="ml-4 list-disc space-y-2">
            <li>
              <strong>
                A community app misbehaves, or you have an idea for one:
              </strong>{" "}
              open an issue in{" "}
              <ExternalLink href={`${COMMUNITY_REPO}/issues`}>
                faderpunk-community-apps
              </ExternalLink>
              .
            </li>
            <li>
              <strong>Firmware or Configurator problem:</strong> open an issue
              in{" "}
              <ExternalLink href={`${FIRMWARE_REPO}/issues`}>
                faderpunk
              </ExternalLink>
              . Mention whether a community app was installed.
            </li>
            <li>
              <strong>Questions and general chat:</strong> join the{" "}
              <ExternalLink href="https://atov.de/discord">
                ATOV Discord
              </ExternalLink>
              .
            </li>
          </ul>
          <p>
            Prebuilt <code>.fpapp</code> files are on the{" "}
            <ExternalLink href={`${COMMUNITY_REPO}/releases`}>
              releases page
            </ExternalLink>
            .
          </p>
        </Card>
      </div>

      {apps ? (
        <CommunityApps apps={apps} renderAction={renderManualLink} />
      ) : null}
      <InstalledApps />

      <div className="border-t border-white/10 pt-10">
        <h2 className="text-yellow-fp mb-2 text-xl font-bold">
          Community Manual
        </h2>
        <CommunityCatalogue />
        <CommunityTools />
      </div>
    </div>
  );
};
