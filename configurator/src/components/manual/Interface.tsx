import {
  Table,
  TableHeader,
  TableColumn,
  TableBody,
  TableRow,
  TableCell,
} from "@heroui/table";
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

    <p className="mt-4">
      When adjusting the <strong>quantizer scale</strong> (Fader 4) or{" "}
      <strong>quantizer root note</strong> (Fader 5), the LED below the
      respective fader displays a color indicating the current selection. This
      provides immediate visual feedback for the active scale or tonic.
    </p>

    <Table
      aria-label="Quantizer color mapping"
      className="my-4"
      isStriped
      classNames={{
        wrapper: "bg-transparent shadow-none",
      }}
    >
      <TableHeader>
        <TableColumn>COLOR</TableColumn>
        <TableColumn>KEY</TableColumn>
        <TableColumn>TONIC</TableColumn>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-white h-5 w-5 border border-gray-300" />
              White
            </div>
          </TableCell>
          <TableCell>Chromatic</TableCell>
          <TableCell>C</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-pink h-5 w-5" />
              Pink
            </div>
          </TableCell>
          <TableCell>Ionian</TableCell>
          <TableCell>C#</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-yellow h-5 w-5" />
              Yellow
            </div>
          </TableCell>
          <TableCell>Dorian</TableCell>
          <TableCell>D</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-cyan h-5 w-5" />
              Cyan
            </div>
          </TableCell>
          <TableCell>Phrygian</TableCell>
          <TableCell>D#</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-salmon h-5 w-5" />
              Salmon
            </div>
          </TableCell>
          <TableCell>Lydian</TableCell>
          <TableCell>E</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-lime h-5 w-5" />
              Lime
            </div>
          </TableCell>
          <TableCell>Mixolydian</TableCell>
          <TableCell>F</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-orange h-5 w-5" />
              Orange
            </div>
          </TableCell>
          <TableCell>Aeolian</TableCell>
          <TableCell>F#</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-green h-5 w-5" />
              Green
            </div>
          </TableCell>
          <TableCell>Locrian</TableCell>
          <TableCell>G</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-sky-blue h-5 w-5" />
              Sky Blue
            </div>
          </TableCell>
          <TableCell>Blues Major</TableCell>
          <TableCell>G#</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-red h-5 w-5" />
              Red
            </div>
          </TableCell>
          <TableCell>Blues Minor</TableCell>
          <TableCell>A</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-pale-green h-5 w-5" />
              Pale Green
            </div>
          </TableCell>
          <TableCell>Pentatonic Major</TableCell>
          <TableCell>A#</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-blue h-5 w-5" />
              Blue
            </div>
          </TableCell>
          <TableCell>Pentatonic Minor</TableCell>
          <TableCell>B</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-sand h-5 w-5" />
              Sand
            </div>
          </TableCell>
          <TableCell>Folk</TableCell>
          <TableCell>—</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-violet h-5 w-5" />
              Violet
            </div>
          </TableCell>
          <TableCell>Japanese</TableCell>
          <TableCell>—</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-light-blue h-5 w-5" />
              Light Blue
            </div>
          </TableCell>
          <TableCell>Gamelan</TableCell>
          <TableCell>—</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>
            <div className="flex items-center gap-2">
              <div className="bg-palette-rose h-5 w-5" />
              Rose
            </div>
          </TableCell>
          <TableCell>Hungarian Minor</TableCell>
          <TableCell>—</TableCell>
        </TableRow>
      </TableBody>
    </Table>

    <H3>Back Connectors</H3>
    <p>
      Faderpunk features a set of connectors on the rear panel, designed to
      support power, communication, and integration with other gear. The ability
      to route and filter <strong>MIDI</strong> signals from inputs to output as
      well as the ability to assign the <strong>apps</strong> to specific
      physical <strong>IN/OUTs</strong> makes Faderpunk a very powerfull{" "}
      <strong>MIDI router</strong>.
    </p>
    <List>
      <li>
        <strong>USB</strong>
        <br />
        This port powers the unit and enables bi‑directional MIDI communication
        as well as connection to the online Configurator. It can transmit all
        MIDI data generated by the <strong>apps</strong> (LFOs, envelopes,
        modulation sources, etc.) and can also receive external{" "}
        <strong>MIDI</strong> that can be routed internally using the{" "}
        <strong>MIDI</strong> settings in the <strong>Configurator</strong>{" "}
        section.
      </li>

      <li>
        <strong>I²C</strong>
        <br />
        I²C is a digital communication protocol used by modules such as the
        Orthogonal Devices ER‑301, Monome Teletype, and Expert Sleepers Disting
        EX.
        <br />
        Faderpunk can operate as either a <strong>Leader</strong> or{" "}
        <strong>Follower</strong> on the I²C bus, and can send or receive
        parameter changes depending on the selected routing mode.
        <br />
      </li>

      <li>
        <strong>MIDI In</strong>
        <br />
        A 3.5mm stereo jack that accepts incoming MIDI data.
        <br />
        This connector is <strong>polarity agnostic</strong> and supports both
        Type A and Type B MIDI standards.
        <br />
        Incoming data from this port can be routed to the outputs (MIDI out 1 &
        2 as well as USB) according to the settings in the <strong>
          MIDI
        </strong>{" "}
        section of the <strong>Configurator</strong>.
      </li>

      <li>
        <strong>MIDI Out 1 &amp; Out 2</strong>
        <br />
        These 3.5mm stereo jacks transmit MIDI data from Faderpunk.
        <br />
        The data sent depends on the selected routing mode:
        <strong>Local</strong> (apps only), <strong>MIDI Thru</strong> (external
        input only),
        <strong>MIDI Merge</strong> (combined), or <strong>None</strong>.
        <br />
        This configuration is independent for each output.
        <br />
        When using Faderpunk’s <strong>internal clock</strong>, MIDI Clock is
        sent through these outputs as well if enabled in the clock routing
        settings.
        <br />
        These connectors follow the <strong>Type A</strong> MIDI standard.
      </li>
    </List>

    <H3>Internal Connectors Overview</H3>
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
        to compatible IO boards found in Intellijel or Befaco cases.
      </li>
      <li>
        <strong>I²C Connector</strong>
        <br />
        Enables communication with I²C-compatible devices while Faderpunk is
        mounted inside a case. This allows interaction with devices like Monome
        Teletype, ER-301, and others.
      </li>
      <li>
        <strong>Programming Header (SWD, GND, SWCLK)</strong>
        <br />
        Used for firmware flashing and debugging via a compatible debug probe
        (e.g., Raspberry Pi Debug Probe), allowing for faster development and
        troubleshooting cycles. Please note that the labels on the PCB are
        inverted (SWD is SWCLK and SWCLK is SWD)
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
