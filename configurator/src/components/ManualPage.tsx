import { Layout } from "./Layout";
import { type ManualAppData, ManualApp } from "./ManualApp";

const apps: ManualAppData[] = [
  {
    appId: 1,
    title: "Control",
    description: "Simple MIDI/CV controller",
    color: "Violet",
    icon: "fader",
    text: "Long description manual text",
    channels: [
      {
        jackTitle: "Output",
        jackDescription: "CV Output value in given range.",
        faderTitle: "CV and MIDI value",
        faderDescription: "Determines CV and Midi value.",
        faderPlusShiftTitle: "Attenuation",
        faderPlusShiftDescription: "Attenuation for chosen range.",
        fnTitle: "Mute",
        fnDescription: "Mute CV and MIDI output.",
        ledTop: "Current value",
        ledTopPlusShift: "Attenuation",
        ledBottom: "Current value",
      },
    ],
  },
];

export const ManualPage = () => (
  <Layout>
    <h2 className="text-yellow-fp mb-4 text-lg font-bold uppercase">Preface</h2>
    <p>Here is some preface text</p>
    <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
      Apps
    </h2>
    <nav className="mb-8">
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
  </Layout>
);
