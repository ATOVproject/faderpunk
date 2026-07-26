import { useEffect, useState } from "react";

import type { DeviceInfo } from "../mapping/tracks";
import { liveStats } from "../midi/live-stats";

export type DeviceLiveStats = {
  usbOn: number;
  usbCapable: number;
  transportRunning: boolean;
  monitorOn: boolean;
};

type Props = {
  info: DeviceInfo | null;
  live?: DeviceLiveStats | null;
};

function Chip({
  children,
  title,
  live,
}: {
  children: string;
  title?: string;
  live?: boolean;
}) {
  return (
    <span className={`device-chip${live ? " live" : ""}`} title={title ?? children}>
      {children}
    </span>
  );
}

const STATS_POLL_MS = 120;

/** Poll high-rate MIDI counters without tying AppShell to every clock/CC. */
function useLiveMidiStats(active: boolean) {
  const [stats, setStats] = useState(() => ({ ...liveStats }));
  useEffect(() => {
    if (!active) return;
    const tick = () => {
      if (document.hidden) return;
      setStats({
        clockCount: liveStats.clockCount,
        ccCount: liveStats.ccCount,
        noteCount: liveStats.noteCount,
        loopbackCount: liveStats.loopbackCount,
      });
    };
    tick();
    const id = window.setInterval(tick, STATS_POLL_MS);
    const onVis = () => {
      if (!document.hidden) tick();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [active]);
  return stats;
}

export function DevicePanel({ info, live }: Props) {
  const counters = useLiveMidiStats(Boolean(live));

  return (
    <section className="device-panel" aria-label="Device">
      <h2>Device</h2>

      {live && (
        <div className="device-chips device-live-chips" aria-label="Session">
          <Chip title="Apps with MidiOut→USB / capable">
            {`usb ${live.usbOn}/${live.usbCapable}`}
          </Chip>
          <Chip>{`${counters.ccCount} cc`}</Chip>
          <Chip>{`${counters.noteCount} notes`}</Chip>
          <Chip>{`${counters.clockCount} clocks`}</Chip>
          <Chip live>{`echo ${counters.loopbackCount}`}</Chip>
          <Chip live={live.transportRunning}>
            {live.transportRunning ? "running" : "stopped"}
          </Chip>
          <Chip live={live.monitorOn}>
            {live.monitorOn ? "monitor on" : "all muted"}
          </Chip>
        </div>
      )}

      {!info && <p className="device-empty">No GlobalConfig yet</p>}

      {info && (
        <>
          <div className="device-chips">
            {(
              [
                `fw ${info.version}`,
                info.clockSrc,
                `${info.bpm} BPM`,
                info.swing === 0
                  ? "swing 0"
                  : info.swing > 0
                    ? `swing +${info.swing}`
                    : `swing ${info.swing}`,
                info.resetSrc !== "None" ? `rst ${info.resetSrc}` : null,
                `PPQN ${info.extPpqn}`,
                `I2C ${info.i2c}`,
                `LED ${info.ledBrightness}`,
                info.takeover,
                `Q ${info.quantizer}`,
                `Atom ${info.aux.atom}`,
                `Meteor ${info.aux.meteor}`,
                `Cube ${info.aux.cube}`,
              ] as (string | null)[]
            )
              .filter((c): c is string => Boolean(c))
              .map((c) => (
                <Chip key={c}>{c}</Chip>
              ))}
          </div>
          <div className="device-chips device-midi-chips">
            {info.midiOuts.map((out) => {
              const flags = [out.sendClock ? "clk" : null, out.sendTransport ? "tr" : null]
                .filter(Boolean)
                .join("/");
              const text = flags
                ? `${out.label} ${out.mode} ${flags}`
                : `${out.label} ${out.mode}`;
              return <Chip key={out.label}>{text}</Chip>;
            })}
          </div>
        </>
      )}
    </section>
  );
}
