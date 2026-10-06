import clx from "classnames";
import type { ReactNode } from "react";

import { Icon } from "../components/Icon";
import { COLORS_CLASSES } from "../utils/class-helpers";

export interface AppGridItem {
  id: number;
  name: string;
  description: ReactNode;
  color: string;
  icon: string;
  href: string;
}

// Shared by the official /apps/ embed and the community catalogue's embed
// page (built in faderpunk-community-apps, which imports this file through
// its .faderpunk symlink) so both stay visually identical. That build renders
// it to static HTML in Node, so keep it free of browser-only APIs.
export function EmbedPage({ children }: { children: ReactNode }) {
  return <main className="bg-gray-500 px-4 py-8 text-white">{children}</main>;
}

export function AppGrid({ apps }: { apps: AppGridItem[] }) {
  return (
    <ul className="mx-auto grid max-w-4xl grid-cols-1 gap-x-12 gap-y-4 sm:grid-cols-2">
      {apps.map((app) => (
        <li key={app.id}>
          <a
            href={app.href}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-4"
          >
            <div
              className={clx(
                "flex h-14 w-14 shrink-0 items-center justify-center rounded-sm p-2",
                COLORS_CLASSES[app.color as keyof typeof COLORS_CLASSES]?.bg,
              )}
            >
              <Icon className="h-8 w-8 text-black" name={app.icon} />
            </div>
            <div>
              <h2 className="text-yellow-fp font-bold uppercase">{app.name}</h2>
              <p className="text-sm text-white/80">{app.description}</p>
            </div>
          </a>
        </li>
      ))}
    </ul>
  );
}
