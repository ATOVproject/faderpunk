import { type ManualAppData, ManualApp } from "./ManualApp";
import { H2, H3, List } from "./Shared";

interface Props {
  apps: ManualAppData[];
}

export const Apps = ({ apps }: Props) => (
  <>
    <H2 id="apps">Apps</H2>
    <p className="mb-6">
      Across the app library, a handful of controls follow shared
      conventions—the same gesture or LED color tends to mean the same thing
      from one app to the next. We've made our best effort to keep this
      consistent, but the limited number of physical controls per channel means
      a few apps use a different gesture for the same idea, or reuse a gesture
      for something else entirely. The sections below cover the most common
      conventions; each app's own entry documents its exact behavior.
    </p>
    <H3 id="attenuation-apps">Attenuation</H3>
    <p>
      Most apps that produce a CV or MIDI CC output include a built-in
      attenuator. Holding <strong>Shift</strong> and moving the fader on the
      app's output channel scales the output down—toward 0 V for unipolar
      ranges, or toward center for bipolar ranges—without moving the fader's own
      stored value. Both the CV and any corresponding MIDI CC are scaled
      together, and the attenuation level is saved per-scene.
    </p>
    <p className="mt-2">
      This applies to: Control, LFO, LFO+, AD Envelope, Random CC/CV, Random+,
      Turing, and Panner.
    </p>
    <p className="mt-2">
      A couple of apps handle it differently: <strong>Turing+</strong> dedicates
      its output channel's main fader to attenuation directly, with no Shift
      needed. <strong>Automator</strong>'s Shift + Fader only attenuates the
      recorded loop, leaving the passthrough/offset fader unaffected.
    </p>
    <p className="mt-2">
      While Shift is held, the channel's top LED turns red and its brightness
      reflects the current attenuation level.
    </p>
    <p className="mt-2 mb-8">
      A few apps go further with a full <strong>attenuverter</strong>, which can
      also invert the signal and boost it above unity (up to 2x gain) rather
      than only attenuating: <strong>Offset + Attenuverter</strong> (direct
      fader), and <strong>Slew Limiter</strong> and{" "}
      <strong>Envelope Follower</strong> (Shift + Fader). On Envelope Follower,
      Shift + short press on the attenuverter's button resets it back to unity
      gain.
    </p>
    <H3 id="muting-apps">Muting apps</H3>
    <p>
      Most apps support muting their output. When muted, the output is held at a
      neutral voltage — 0 V for unipolar outputs (0 to 10 V and 0 to 5V range),
      or the midpoint (0 V) for bipolar outputs (−5 to +5 V range). MIDI output
      is also suppressed. Mute state is saved per-scene and survives power
      cycles.
    </p>
    <p className="mt-2">The gesture depends on the app:</p>
    <List>
      <li>
        <strong>Short press (no shift)</strong> — Control (when Button mode =
        Mute), Clock Divider, Clock Divider+, Random CC/CV, Random+ (output
        channel), Random Trigger, Euclid, Envelope Follower, Turing, Turing+,
        MIDI to CV, CV2MIDI, CV/OCT to MIDI, Panner, FP-Grids (per-channel
        trigger mutes), TB-3PO, GenSeq, Bernoulli Gate (button 1 mutes Output A,
        button 2 mutes Output B)
      </li>
      <li>
        <strong>Long press (no shift)</strong> — AD Envelope, LFO, LFO+, Super
        LFO (left button mutes CV in, right button mutes CV out)
      </li>
      <li>
        <strong>Shift + long press on button 0 / 2 / 4 / 6</strong> — Sequencer
        (mutes track 1 / 2 / 3 / 4 respectively)
      </li>
    </List>
    <p className="mb-8">
      The button LED turns off when muted and lights up again when unmuted.
    </p>
    <H3 id="resolution-apps">Clock Resolution</H3>
    <p>
      Clocked apps let you pick a note division for their internal timing—from
      fast triplet subdivisions up to multiple bars—independent of the fader's
      primary function. Unlike attenuation, the gesture used to reach this
      control isn't consistent across apps:
    </p>
    <List>
      <li>
        <strong>Shift + Fader</strong> — Random Trigger, Note Fader, FP-Grids
        (Euclidean mode only), Sequencer (Shift + Fader 5, part of the 8-fader
        shift layer)
      </li>
      <li>
        <strong>Fn (Button) + Fader</strong> — Turing, Euclid, TB-3PO, GenSeq
      </li>
    </List>
    <p className="mt-2">
      A few apps skip the modifier entirely and use the main fader directly:{" "}
      <strong>Clock Divider</strong> and <strong>Clock Divider+</strong> (Shift
      / Fn + Fader instead set the maximum / minimum of the divider's range),{" "}
      <strong>LFO</strong> and <strong>LFO+</strong> (the speed fader snaps to a
      resolution once clocked), and <strong>Bernoulli Gate</strong>.
    </p>
    <p className="mt-2 mb-8">
      Across these apps, the LED marking the current division is{" "}
      <strong className="text-palette-orange">orange</strong> for triplet
      divisions and <strong className="text-cyan-fp">blue</strong> for straight
      divisions.
    </p>
    {apps.map((app) => (
      <ManualApp key={app.appId} app={app} />
    ))}
  </>
);
