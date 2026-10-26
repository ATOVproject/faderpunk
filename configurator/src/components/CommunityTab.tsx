import type { PropsWithChildren } from "react";

import { CommunityCatalogue } from "./manual/CommunityCatalogue";
import { CommunityTools } from "./manual/CommunityTools";

const COMMUNITY_REPO =
  "https://github.com/ATOVproject/faderpunk-community-apps";
const FIRMWARE_REPO = "https://github.com/ATOVproject/faderpunk";

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

export const CommunityTab = () => (
  <div>
    <div className="mb-12 max-w-3xl">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Community Apps — Unofficial, Unmaintained
      </h2>
      <p className="mb-3">
        Community apps are built and shared by Faderpunk players and developers.
        They install into a free slot from the Apps tab without reflashing
        firmware, and sit next to the official apps in your layout.
      </p>
      <p>
        They are not made, reviewed or supported by ATOV, and come without
        guarantees: install only apps you trust. If one ever stops the device
        responding, a factory reset removes it (see the Troubleshooting section
        of the Manual).
      </p>
      <p className="mt-3">
        Looking for tools rather than apps? Kosmar's{" "}
        <ExternalLink href="https://kosmar.github.io/faderpunk-tools/">
          Faderpunk-tools
        </ExternalLink>{" "}
        offers Presetpunk (save and recall full layouts as presets) and
        Scopepunk (a MIDI output visualizer for app developers). It's a
        third-party project, not supported by ATOV.
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
            one entry in <code>apps-catalog.json</code> — its catalogue listing
          </li>
        </ul>
        <p>
          Automated checks run on every pull request, and a maintainer makes the
          final merge decision. Read the{" "}
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
            <strong>Firmware or Configurator problem:</strong> open an issue in{" "}
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

    <div className="border-t border-white/10 pt-10">
      <h2 className="text-yellow-fp mb-2 text-xl font-bold">
        Community Manual
      </h2>
      <CommunityCatalogue />
      <CommunityTools />
    </div>
  </div>
);
