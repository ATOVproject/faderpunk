import { H2, H3, H4, List } from "./Shared";

export const Configurator = () => (
  <>
    <H2 id="configurator">Faderpunk Configurator</H2>
    <p>
      <strong>The Configurator is a core component of Faderpunk.</strong>
      <br />
      It's the tool you'll use to create app layouts, edit app parameters, and
      modify internal settings of the Faderpunk device.
    </p>
    <p>
      The Configurator uses <strong>WebUSB</strong>, a communication protocol
      that is not supported by all browsers. To access it, you'll need a{" "}
      <strong>Chromium-based browser</strong>.
    </p>

    <H3>Compatible Browsers with WebUSB Support:</H3>
    <List>
      <li>Google Chrome</li>
      <li>Microsoft Edge</li>
      <li>Vivaldi</li>
      <li>Brave</li>
      <li>Opera</li>
      <li>Chromium (open-source base)</li>
    </List>

    <p>
      To get started, open a WebUSB-compatible browser and visit{" "}
      <a className="font-semibold underline" href="https://faderpunk.io">
        https://faderpunk.io
      </a>
      .
      <br />
      Upon visiting the site, you'll be prompted to connect a device. Click{" "}
      <strong>"Connect Device"</strong>, and a pop-up will appear allowing you
      to select and connect to your Faderpunk.
    </p>
    <p>
      Once connected, you'll be greeted by the Configurator interface, which
      consists of <strong>three tabs</strong>:
    </p>
    <List>
      <li>
        <strong>Device Tab</strong> – Edit the layout and adjust parameters of
        the apps loaded in your configuration.
      </li>
      <li>
        <strong>Apps Tab</strong> – Choose which apps to include in your layout.
      </li>
      <li>
        <strong>Settings Tab</strong> – Modify global configurations of the
        Faderpunk device.
      </li>
    </List>

    <H3>Device Tab</H3>

    <img
      className="my-6"
      alt="Screenshot of the device overview in the Faderpunk configurator"
      src="/img/configurator-device.png"
    />

    <p>The Device tab is divided into two sections:</p>

    <H4>Channel Overview</H4>
    <p>
      At the top, you'll find the <strong>Channel Overview</strong>, which
      provides a visual representation of the apps currently loaded on your
      Faderpunk.
    </p>
    <p>
      Clicking on this graphic opens the <strong>Edit Layout</strong> interface.
      Here, you can drag and drop apps to assign them to different channels.
      Clicking on an app within the layout editor allows you to{" "}
      <strong>remove</strong> it from the layout.
    </p>
    <p>
      To confirm your changes, click <strong>Save</strong>. This extra step
      helps prevent accidental modifications and loss of work.
      <br />
      Clicking <strong>Save</strong> will apply the new layout, while{" "}
      <strong>Cancel</strong> will discard the changes and return you to the
      main Device tab.
    </p>

    <H4>Active Apps</H4>
    <p>This section lets you edit the parameters of each loaded app.</p>
    <p>
      All apps currently installed on your Faderpunk are listed here in order.
      Each entry displays the app's name, its assigned channel, and the number
      of slots it occupies.
    </p>
    <p>
      To edit an app's parameters, simply click on it. A menu will appear below,
      allowing you to adjust its settings.
    </p>
    <p>
      Just like with the layout, you must click <strong>Save</strong> to apply
      changes to each individual app. This safeguard ensures that no unintended
      modifications are made.
    </p>

    <H4>Save & Load Layout</H4>
    <p>
      Below the Active Apps section, you'll find controls for saving and loading
      complete layouts.
    </p>
    <p>
      <strong>Saving a Layout</strong>
      <br />
      To save your current configuration:
    </p>
    <List>
      <li>
        Enter a name for your layout in the File name field (defaults to
        "faderpunk-layout")
      </li>
      <li>
        Click <strong>Save current layout</strong>
      </li>
    </List>
    <p>
      This saves both your channel layout and all app parameters as a .json
      file, which will be downloaded to your computer. This is particularly
      useful for:
    </p>
    <List>
      <li>Creating backup configurations</li>
      <li>Setting up different performance layouts</li>
      <li>Sharing configurations between devices</li>
    </List>
    <p>
      <strong>Loading a Layout</strong>
      <br />
      To load a previously saved layout:
    </p>
    <List>
      <li>
        Click <strong>Choose layout file</strong> and select a .json layout file
        from your computer
      </li>
      <li>The selected filename will appear next to the button</li>
      <li>
        Click <strong>Load</strong> to apply the layout, or{" "}
        <strong>Cancel</strong> to abort
      </li>
    </List>
    <p>
      Loading a layout will replace your current channel assignments and app
      parameters with those stored in the file. Make sure to save your current
      layout first if you want to preserve it.
    </p>

    <H3>Apps Tab</H3>

    <img
      className="my-6"
      alt="Screenshot of the apps tab in the Faderpunk configurator"
      src="/img/configurator-apps.png"
    />

    <p>
      The Apps tab is where you select the apps you'd like to include in your
      layout. Like the Device tab, it consists of two sections:
    </p>

    <H4>Channel Overview</H4>
    <p>
      This section is identical to the one in the Device tab. It provides a
      visual representation of your current layout and allows you to rearrange
      apps across channels.
    </p>

    <H4>Available Apps List</H4>
    <p>
      Apps are listed here in order of the number of channels they use, followed
      by alphabetical order.
    </p>
    <p>
      Clicking on an app opens the <strong>Add App</strong> pop-up, where you
      can place the selected app into your layout. This pop-up also displays:
    </p>
    <List>
      <li>The app's available parameters</li>
      <li>The number of channels it occupies</li>
      <li>A link to the app's manual</li>
    </List>
    <p>
      Within the Add App interface, you can drag and drop not only the new app
      but also the apps already loaded on your Faderpunk. This allows for
      flexible layout adjustments.
    </p>
    <p>
      If there are no available channels for the selected app, you'll see the
      message:
    </p>
    <p>
      <strong>
        "I can't find space for the app. Try to remove apps or move them
        around."
      </strong>
    </p>
    <p>
      In this case, you can rearrange or delete apps just as you would in the
      Edit Layout pop-up.
    </p>

    <H3>Settings Tab</H3>

    <img
      className="my-6"
      alt="Screenshot of the settings tab in the Faderpunk configurator"
      src="/img/configurator-settings.png"
    />

    <p>
      In this tab, you can edit the <strong>global parameters</strong> of your
      Faderpunk device.
    </p>

    <H4>Clock Section</H4>
    <p>Here you can configure the clock behavior:</p>
    <List>
      <li>
        <strong>Clock Source</strong>: Choose between:
        <List>
          <li>Internal</li>
          <li>MIDI-In (3.5mm jack)</li>
          <li>MIDI USB</li>
          <li>
            Analog AUX jacks (Atom, Meteor, Cube) on the right side of the
            device
          </li>
        </List>
      </li>
    </List>
    <p>
      ⚠️ Currently, the only supported analog clock input resolution is{" "}
      <strong>24 PPQN</strong>. We're actively working on supporting additional
      resolutions.
    </p>
    <List>
      <li>
        <strong>Reset Source</strong>: Select from:
        <List>
          <li>None</li>
          <li>Atom</li>
          <li>Meteor</li>
          <li>Cube</li>
        </List>
      </li>
    </List>
    <p>
      You can use these AUX jacks as reset sources even when syncing to MIDI or
      the internal clock.
    </p>
    <List>
      <li>
        <strong>BPM</strong>: Set the BPM for the internal clock.
        <br />
        You can also adjust BPM manually using <strong>Scene + Fader 16</strong>
        .
      </li>
    </List>

    <H4>Quantizer</H4>
    <p>Configure the internal quantizer used across all apps:</p>
    <List>
      <li>
        <strong>Scale</strong> and <strong>Tonic</strong> can be set here.
      </li>
      <li>
        To adjust manually:
        <List>
          <li>
            Change <strong>Scale</strong> with <strong>Scene + Fader 4</strong>
          </li>
          <li>
            Change <strong>Tonic</strong> with <strong>Scene + Fader 5</strong>
          </li>
        </List>
      </li>
    </List>
    <p>The following scales are available:</p>
    <List>
      <li>Chromatic</li>
      <li>Ionian</li>
      <li>Dorian</li>
      <li>Phrygian</li>
      <li>Lydian</li>
      <li>Mixolydian</li>
      <li>Aeolian</li>
      <li>Locrian</li>
      <li>Blues Major</li>
      <li>Blues Minor</li>
      <li>Pentatonic Major</li>
      <li>Pentatonic Minor</li>
      <li>Folk</li>
      <li>Japanese</li>
      <li>Gamelan</li>
      <li>Hungarian Minor</li>
    </List>
    <p>Refer to each app's manual to check if it uses the global quantizer.</p>

    <H4>I²C Configuration</H4>
    <p>
      Faderpunk can operate as either a <strong>Leader</strong> or{" "}
      <strong>Follower</strong> on the I²C bus.
      <br />
      You can set this behavior in the Settings tab.
    </p>

    <H4>AUX Jacks</H4>
    <p>
      Configure AUX jacks as <strong>clock outputs</strong> or{" "}
      <strong>reset outputs</strong>.
    </p>
    <p>
      <strong>Available clock output resolutions:</strong>
    </p>
    <List>
      <li>24 PPQN</li>
      <li>12 PPQN</li>
      <li>6 PPQN</li>
      <li>4 PPQN</li>
      <li>3 PPQN</li>
      <li>2 PPQN</li>
      <li>1 PPQN</li>
      <li>1 bar</li>
      <li>2 bars</li>
      <li>4 bars</li>
    </List>

    <H4>Miscellaneous</H4>
    <List>
      <li>
        <strong>LED Brightness</strong>: Adjust the brightness of the device's
        LEDs.
        <br />
        You can also change this manually using <strong>Scene + Fader 1</strong>
        .
      </li>
    </List>
  </>
);
