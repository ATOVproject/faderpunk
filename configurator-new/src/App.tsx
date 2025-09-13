const App = () => {
  return (
    <div className="min-h-screen bg-gray-900 text-white">
      <div className="mx-auto max-w-6xl p-8">
        <div className="mb-8 text-center">
          <h1 className="font-vox mb-2 text-4xl font-bold">
            <span className="text-yellow-400">FADER</span>
            <span className="text-purple-400">PUNK</span>
          </h1>
          <p className="font-vox tracking-wider text-gray-400 uppercase"></p>
        </div>

        <div className="mb-8 flex border-b border-gray-700">
          <div className="border-b-2 border-white bg-gray-800 px-8 py-3">
            DEVICE
          </div>
          <div className="px-8 py-3 text-gray-500">APPS</div>
          <div className="px-8 py-3 text-gray-500">SETTINGS</div>
        </div>

        <div className="mb-8">
          <h2 className="font-vox mb-4 text-sm text-yellow-500 uppercase">
            Channel Overview
          </h2>
          <div className="flex gap-2">
            <div className="flex-2 rounded bg-red-600 px-6 py-4 text-center">
              1-2
            </div>
            <div className="flex-1 rounded bg-blue-500 px-6 py-4 text-center">
              3
            </div>
            <div className="flex-8 rounded bg-yellow-700 px-6 py-4 text-center">
              4-11
            </div>
            <div className="flex-1 rounded bg-black px-6 py-4 text-center">
              13
            </div>
            <div className="flex-1 rounded bg-green-600 px-6 py-4 text-center">
              14
            </div>
            <div className="flex-2 rounded bg-purple-600 px-6 py-4 text-center">
              15-16
            </div>
          </div>
        </div>

        <div>
          <h2 className="font-vox mb-4 text-sm text-yellow-500 uppercase">
            Active Apps
          </h2>

          <div className="space-y-2">
            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-red-600"></div>
              <div className="flex-1">
                <p className="text-xs text-yellow-500 uppercase">App</p>
                <p>AD Envelope</p>
              </div>
              <div className="flex-1">
                <p className="text-xs text-yellow-500 uppercase">Channels</p>
                <p>1-2</p>
              </div>
              <div className="flex-1">
                <p className="text-xs text-yellow-500 uppercase">Span</p>
                <p>2</p>
              </div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-blue-500"></div>
              <div className="flex-1">Default</div>
              <div className="flex-1">3</div>
              <div className="flex-1">1</div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-yellow-700"></div>
              <div className="flex-1">Sequencer</div>
              <div className="flex-1">4-11</div>
              <div className="flex-1">8</div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-green-600"></div>
              <div className="flex-1">Automator</div>
              <div className="flex-1">12</div>
              <div className="flex-1">1</div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-blue-500"></div>
              <div className="flex-1">Default</div>
              <div className="flex-1">13</div>
              <div className="flex-1">1</div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-green-600"></div>
              <div className="flex-1">Automator</div>
              <div className="flex-1">14</div>
              <div className="flex-1">1</div>
              <div className="text-2xl">▶</div>
            </div>

            <div className="flex items-center rounded-lg bg-gray-800 p-4">
              <div className="mr-4 h-12 w-12 rounded bg-purple-600"></div>
              <div className="flex-1">Slew Limiter</div>
              <div className="flex-1">15-16</div>
              <div className="flex-1">2</div>
              <div className="text-2xl">▶</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default App;
