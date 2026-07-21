import clx from "classnames";
import { major, minor } from "semver";

import { Icon } from "../components/Icon";
import { COLORS_CLASSES } from "../utils/class-helpers";
import { APP_LIBRARY } from "../generated/appLibrary";
import { useLatestFirmwareVersion } from "../useLatestFirmwareVersion";

function versionPath(version: string) {
  return `/${major(version)}.${minor(version)}/`;
}

export default function App() {
  const latestVersion = useLatestFirmwareVersion();

  return (
    <main className="bg-gray-500 px-4 py-8 text-white">
      <ul className="mx-auto grid max-w-4xl grid-cols-1 gap-x-12 gap-y-4 sm:grid-cols-2">
        {APP_LIBRARY.map((app) => (
          <li key={app.id}>
            <a
              href={`${versionPath(latestVersion)}#/manual#app-${app.id}`}
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
                <h2 className="text-yellow-fp font-bold uppercase">
                  {app.name}
                </h2>
                <p className="text-sm text-white/80">{app.description}</p>
              </div>
            </a>
          </li>
        ))}
      </ul>
    </main>
  );
}
