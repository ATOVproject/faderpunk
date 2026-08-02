import { H2, H3, List } from "./Shared";

export const PunkBus = () => (
  <>
    <H2 id="punkbus">PunkBus</H2>
    <img
      className="my-6 max-w-xs"
      alt="PunkBus Eurorack breakout module for Faderpunk"
      src="/img/punkbus.jpg"
    />
    <p>
      PunkBus is an optional Eurorack breakout module for Faderpunk. It replaces
      19 individual patch cables with a single connection, speeding up setup for
      live performances and cutting cable clutter on stage or in the studio. Two
      PunkBus units can also be linked together as a multicore connection
      between two Eurorack cases. Below is its manual, also available at{" "}
      <a
        className="font-semibold underline"
        href="https://atov.de/pages/punkbus-manual"
        target="_blank"
        rel="noreferrer"
      >
        atov.de/pages/punkbus-manual
      </a>
      .
    </p>

    <H3>Introduction</H3>
    <p>PunkBus connects all 19 Faderpunk jacks with a single cable.</p>

    <H3>⚠️ Warning</H3>
    <p>
      The HDMI connector carries CV/gate signals, not video. Never connect
      PunkBus or Faderpunk to a TV, monitor, or other HDMI video equipment —
      this can cause serious damage to both devices.
    </p>

    <H3>In the Box</H3>
    <List>
      <li>PunkBus module</li>
      <li>Micro HDMI to HDMI cable (1 m)</li>
    </List>

    <H3>Setup</H3>
    <ol className="my-3 ml-3 list-inside list-decimal">
      <li>
        Check your Faderpunk's hardware version on the serial number sticker, on
        the bottom of the case.
      </li>
      <li>
        Set the switch on the back of PunkBus to match: <strong>V1</strong> or{" "}
        <strong>V1.1+</strong>.
      </li>
      <li>
        Connect the micro HDMI end of the cable to the PunkBus connector on the
        back of Faderpunk.
      </li>
      <li>Connect the other end (HDMI) to PunkBus.</li>
      <li>
        Patch as normal, all 19 jacks are now live on PunkBus, color-coded to
        match Faderpunk's panel graphics for easy identification.
      </li>
    </ol>

    <H3>Multicore (Optional)</H3>
    <p>
      Link two PunkBus units with your own HDMI 2.0+ (4K-rated) cable to connect
      two Eurorack cases. Longer cables may reduce CV accuracy.
    </p>
  </>
);
