import ReactMarkdown from "react-markdown";

import { InstalledCommunityApps } from "./InstalledCommunityApps";
import { type CachedFpAppManual } from "../../utils/fpapp";
import {
  withoutHeading,
  type InstalledManualView,
} from "../../hooks/useInstalledManualViews";

const LegacyInstalledAppManuals = ({
  manuals,
}: {
  manuals: CachedFpAppManual[];
}) => (
  <>
    {manuals.map((manual) => (
      <section className="mb-16" id={`app-${manual.appId}`} key={manual.appId}>
        <div className="mb-4">
          <h3 className="text-yellow-fp font-bold uppercase">{manual.name}</h3>
          <p>{manual.description}</p>
        </div>
        {manual.manual && (
          <div className="prose prose-invert max-w-none text-white">
            <ReactMarkdown>
              {withoutHeading(manual.manual, manual)}
            </ReactMarkdown>
          </div>
        )}
        {manual.setup && (
          <details className="mt-6 border-y border-white/10 py-4">
            <summary className="cursor-pointer font-semibold">Setup</summary>
            <div className="prose prose-invert mt-3 max-w-none text-white">
              <ReactMarkdown>{withoutHeading(manual.setup)}</ReactMarkdown>
            </div>
          </details>
        )}
      </section>
    ))}
  </>
);

export const InstalledCommunityManuals = ({
  views,
}: {
  views: InstalledManualView[];
}) => {
  const apps = views.flatMap(({ app }) => (app ? [app] : []));
  const legacy = views.flatMap(({ cached, app }) => (app ? [] : [cached]));
  return (
    <>
      <InstalledCommunityApps apps={apps} />
      <LegacyInstalledAppManuals manuals={legacy} />
    </>
  );
};
