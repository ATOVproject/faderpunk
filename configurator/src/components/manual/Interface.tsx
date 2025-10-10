import { Icon } from "../Icon";
import { H2, H3, List } from "./Shared";

export const Interface = () => (
  <>
    <H2 id="interface">Interface</H2>
    <H3>Front Panel Overview</H3>
    <img
      className="my-6"
      alt="Overview of the Faderpunk panel"
      src="/img/panel.svg"
    />
    <p>
      Faderpunk features 16 identical channels, each designed for flexibility
      and hands-on control. Every channel includes:
    </p>

    <List>
      <li>
        <strong>1x 3.5mm Jack</strong> – Configurable as either input or output,
        depending on the loaded app
      </li>
      <li>
        <strong>1x Fader</strong> – Primary control for modulation or parameter
        adjustment
      </li>
      <li>
        <strong>1x RGB Backlit Function Button</strong> – Used to load and
        interact with apps
      </li>
      <li>
        <strong>2x RGB LEDs</strong> – Positioned next to the fader to provide
        visual feedback
      </li>
    </List>
    <p>
      Apps can be loaded per channel, and some apps span multiple channels
      depending on their complexity.
    </p>
    <H3>Additional Controls (Right Side)</H3>
    <List>
      <li>
        <strong>
          Shift Button (<span className="text-yellow-fp">Yellow</span>)
        </strong>
        <br />
        Located at the bottom, this button enables access to{" "}
        <strong>secondary functions</strong> within apps. These vary depending
        on the app—refer to the individual app manuals for details.
      </li>
      <li>
        <strong>
          Scene Button (<span className="text-pink-fp">Pink</span>)
        </strong>
        <br />
        Positioned above the Shift button, this button is used to{" "}
        <strong>save and recall scenes</strong>:
        <List>
          <li>
            <strong>To save a scene</strong>: Press and hold the Scene button,
            then hold a channel button to store the scene at that location. The
            button will flash <strong>red</strong> to confirm the save.
          </li>
          <li>
            <strong>To recall a scene</strong>: Press the Scene button, then{" "}
            <strong>short press</strong> a channel button to load the saved
            scene. The button will flash <strong>green</strong> to confirm the
            recall.
          </li>
        </List>
        Additionally, holding the <strong>Scene button</strong> while moving
        specific faders gives access to <strong>global parameters</strong>, such
        as:
        <List>
          <li>LED brightness</li>
          <li>Quantizer scale and root note</li>
          <li>BPM (when using internal clock)</li>
        </List>
      </li>
      <li>
        <strong>Analog Clock I/O: Atom, Meteor, and Cube</strong>
        <br />
        On the right side of the device, you'll find{" "}
        <strong>three 3.5mm jack connectors</strong> using the icons:
        <List>
          <li className="flex items-center">
            <Icon name="atom" className="bg-cyan-fp mr-2 h-6 w-6" />
            Atom
          </li>
          <li className="flex items-center">
            <Icon name="meteor" className="bg-yellow-fp mr-2 h-6 w-6" />
            Meteor
          </li>
          <li className="flex items-center">
            <Icon name="cube" className="bg-pink-fp mr-2 h-6 w-6" />
            Cube
          </li>
        </List>
        These jacks are used for <strong>analog clock input and output</strong>,
        and their specific function (e.g., clock in, clock out, reset) is
        configurable via the <strong>Configurator</strong>, allowing flexible
        synchronization with external gear.
      </li>
    </List>

    <H3>Global Parameters Access</H3>
    <p>
      You can adjust several global settings on the Faderpunk by holding the{" "}
      <strong>Scene</strong> button and moving specific faders:
    </p>
    <List>
      <li>
        <strong>Scene + Fader 1</strong> → Adjusts{" "}
        <strong>LED brightness</strong>
      </li>
      <li>
        <strong>Scene + Fader 4</strong> → Sets the{" "}
        <strong>quantizer scale</strong>
      </li>
      <li>
        <strong>Scene + Fader 5</strong> → Sets the{" "}
        <strong>quantizer root note</strong>
      </li>
      <li>
        <strong>Scene + Fader 16</strong> → Controls <strong>BPM</strong> (when
        using the internal clock)
      </li>
    </List>
    <p>Additionally:</p>
    <List>
      <li>
        <strong>Scene + Shift</strong> → <strong>Starts/stops</strong> the
        internal clock
      </li>
    </List>
    <p>
      These shortcuts allow quick access to essential performance parameters
      without needing the configurator, maintaining hands-on control.
    </p>

    <H3>Back Connectors</H3>
    <p>
      Faderpunk features a set of connectors on the rear panel, designed to
      support power, communication, and integration with other gear:
    </p>
    <List>
      <li>
        <strong>USB</strong>
        <br />
        This port provides power to the unit and enables MIDI data transmission
        as well as connection to the online Configurator.
      </li>
      <li>
        <strong>I²C</strong>
        <br />
        I²C is a digital communication protocol used by various modules such as
        the Orthogonal Devices ER-301, Monome Teletype, and Expert Sleepers
        Disting EX.
        <br />
        Faderpunk can operate as either a <strong>Leader</strong> or{" "}
        <strong>Follower</strong> on the I²C bus. You can configure this
        behavior in the <strong>Settings</strong> section of the Configurator.
      </li>
      <li>
        <strong>MIDI In</strong>
        <br />
        A 3.5mm stereo jack that accepts incoming MIDI data.
        <br />
        This connector is <strong>polarity agnostic</strong>, supporting both
        Type A and Type B MIDI standards.
      </li>
      <li>
        <strong>MIDI Out 1 & Out 2</strong>
        <br />
        These 3.5mm stereo jacks transmit MIDI data from the Faderpunk.
        <br />
        Both outputs send the <strong>same MIDI stream</strong>, as defined by
        the active apps.
        <br />
        When Faderpunk is set to use its <strong>internal clock</strong>, MIDI
        clock signals are also sent through these outputs.
        <br />
        These connectors follow the <strong>Type A</strong> MIDI standard.
      </li>
    </List>

    <H3>Rear Connectors Overview</H3>
    <p>
      On the back of the Faderpunk PCB, you'll find a set of user-accessible
      connectors designed to expand functionality and integration:
    </p>
    <List>
      <li>
        <strong>Eurorack Power</strong>
        <br />
        Allows Faderpunk to be powered directly from a Eurorack power supply,
        making it easy to embed into modular systems.
      </li>
      <li>
        <strong>IO Expander (IO EXP)</strong>
        <br />
        Connects to the IO board located at the rear of the Faderpunk case, or
        to compatible IO boards found in Intellijel or Befaco cases. This
        provides USB and MIDI connectivity for seamless integration with
        external gear.
      </li>
      <li>
        <strong>I²C Connector</strong>
        <br />
        Enables communication with I²C-compatible devices while Faderpunk is
        mounted inside a case. This supports modular expansion and interaction
        with devices like Monome Teletype, ER-301, and others.
      </li>
      <li>
        <strong>Programming Header (SWD, GND, SWCLK)</strong>
        <br />
        Used for firmware flashing and debugging via a compatible debug probe
        (e.g., Raspberry Pi Debug Probe), allowing for faster development and
        troubleshooting cycles.
      </li>
      <li>
        <strong>24-Pin Flat Flex Connector</strong>
        <br />
        This connector links the main PCB to the IO board mounted at the back of
        the Faderpunk case.
      </li>
    </List>

    <H3>Important Points</H3>
    <List>
      <li>
        <strong>Configurator Parameters</strong>
        <br />
        The settings available in the Configurator are intended as{" "}
        <strong>"set-and-forget"</strong> options rather than live performance
        controls.
        <br />
        When a parameter is changed, the corresponding app is{" "}
        <strong>reloaded</strong>. If the app is clocked, this reload may cause
        it to fall <strong>out of phase</strong> until it receives a{" "}
        <strong>stop/start</strong> message to resynchronize.
      </li>
      <li>
        <strong>Fader Latching Behavior</strong>
        <br />
        All apps include a feature called <strong>"latching"</strong>, which
        activates when recalling a scene or using a shift function.
        <br />
        If the physical fader position does <strong>not match</strong> the
        stored value, the fader will <strong>not affect</strong> the output
        until it reaches (or "catches") the stored value. This ensures smooth
        transitions and prevents unintended jumps in modulation or control.
      </li>
    </List>
  </>
);
