import { type ManualAppData, ManualApp } from "./ManualApp";
import { H2, H3, List } from "./Shared";

interface Props {
  apps: ManualAppData[];
}

export const Apps = ({ apps }: Props) => (
  <>
    <H2 id="apps">Apps</H2>
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
        button 2 mutes Output B), Echolot
      </li>
      <li>
        <strong>Long press (no shift)</strong> — AD Envelope, LFO, LFO+
      </li>
      <li>
        <strong>Shift + long press on button 0 / 2 / 4 / 6</strong> — Sequencer
        (mutes track 1 / 2 / 3 / 4 respectively)
      </li>
    </List>
    <p className="mb-8">
      The button LED turns off when muted and lights up again when unmuted.
    </p>
    {apps.map((app) => (
      <ManualApp key={app.appId} app={app} />
    ))}
  </>
);
