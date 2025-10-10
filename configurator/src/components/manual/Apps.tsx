import { type ManualAppData, ManualApp } from "./ManualApp";

interface Props {
  apps: ManualAppData[];
}

export const Apps = ({ apps }: Props) => (
  <>
    <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
      Apps
    </h2>
    <nav className="mb-16">
      <ul className="list-inside list-disc">
        {apps.map((app) => (
          <li key={app.title}>
            <a href={`#app-${app.appId}`} className="underline">
              {app.title}
            </a>
          </li>
        ))}
      </ul>
    </nav>
    {apps.map((app) => (
      <ManualApp key={app.appId} app={app} />
    ))}
  </>
);
