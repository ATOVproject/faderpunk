import { COLORS_CLASSES } from "./class-helpers";
import { ChannelOverview } from "./components/ChannelOverview";
import { VariableWidths } from "./components/DnD";
import { Icon } from "./components/Icon";

const ACTIVE_APPS = [
  {
    slots: "1-2",
    channels: 2,
    color: "Yellow",
    name: "AD Envelope",
    description: "Variable curve AD, ASR or looping AD",
    icon: "ad-env",
  },
  {
    slots: "3",
    channels: 1,
    color: "Violet",
    name: "Control",
    description: "Simple MIDI/CV controller",
    icon: "fader",
  },
  {
    slots: "4-11",
    channels: 8,
    color: "Yellow",
    name: "Sequencer",
    description: "4 x 16 step CV/gate sequencers",
    icon: "sequence",
  },
  {
    slots: "12",
    channels: 1,
    color: "None",
    isEmpty: true,
  },
  {
    slots: "13",
    channels: 1,
    color: "Rose",
    name: "Note Fader",
    description: "Play MIDI notes manually or on clock",
    icon: "note",
  },
  {
    slots: "14",
    channels: 1,
    color: "Yellow",
    name: "LFO",
    description: "Multi shape LFO",
    icon: "sine",
  },
  {
    slots: "15-16",
    channels: 2,
    color: "Orange",
    name: "Euclid",
    description: "Euclidean sequencer",
    icon: "euclid",
  },
];

const App = () => {
  return (
    <div className="min-h-screen bg-gray-500 text-white">
      <div className="mx-auto max-w-6xl px-8 py-14">
        <div className="mb-8 text-center">
          <img src="/img/fp-logo.svg" className="inline w-64" />
          <p className="font-vox mt-3 text-xl font-semibold tracking-wider text-white uppercase">
            Configurator
          </p>
        </div>

        <div className="mb-8 flex border-b-3 border-gray-300">
          <div className="rounded-t-md bg-black px-12 py-3 text-lg font-bold text-white uppercase">
            Device
          </div>
          <div className="rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase">
            Apps
          </div>
          <div className="rounded-t-md px-12 py-3 text-lg font-bold text-white uppercase">
            Settings
          </div>
        </div>

        <div className="mb-8">
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Channel Overview
          </h2>
          <ChannelOverview activeApps={ACTIVE_APPS} />
        </div>

        <div>
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Active Apps
          </h2>

          <div className="space-y-2">
            {ACTIVE_APPS.filter((app) => !app.isEmpty).map((app) => {
              return (
                <div className="flex items-center gap-4 bg-black p-4">
                  <div
                    className={`${COLORS_CLASSES[app.color]} h-16 w-16 rounded p-2`}
                  >
                    {app.icon && (
                      <Icon name={app.icon} className="h-full w-full" />
                    )}
                  </div>
                  <div className="flex-1">
                    <p className="text-yellow-fp text-sm font-bold uppercase">
                      App
                    </p>
                    <p className="text-lg font-medium">{app.name}</p>
                  </div>
                  <div className="flex-1">
                    <p className="text-yellow-fp text-sm font-bold uppercase">
                      Channels
                    </p>
                    <p className="text-lg font-medium">{app.slots}</p>
                  </div>
                  <div className="flex-1">
                    <p className="text-yellow-fp text-sm font-bold uppercase">
                      Span
                    </p>
                    <p className="text-lg font-medium">{app.channels}</p>
                  </div>
                  <div className="text-2xl">
                    <Icon name="caret" />
                  </div>
                </div>
              );
            })}
          </div>
        </div>
        <div className="mt-16 border-t-3 border-gray-300">
          <div className="flex items-center justify-between py-8">
            <img src="/img/atov-logo.svg" className="w-16" />
            <div className="flex items-center gap-4">
              <Icon className="h-6 w-6" name="discord" />
              <Icon className="h-6 w-6" name="github" />
              <Icon className="h-6 w-6" name="instagram" />
            </div>
          </div>
        </div>
      </div>
      <VariableWidths />
    </div>
  );
};

export default App;
