import { colorCss, Scope, WaveProfile } from "./Scope";
import type { TrackRuntime } from "../store";
import { useDiag } from "../store";

interface Props {
  runtime: TrackRuntime;
  dimmed?: boolean;
  compact?: boolean;
}

export function TrackPanel({ runtime, dimmed, compact }: Props) {
  const toggleMute = useDiag((s) => s.toggleMute);
  const toggleSolo = useDiag((s) => s.toggleSolo);
  const toggleCompare = useDiag((s) => s.toggleCompare);
  const setFocus = useDiag((s) => s.setFocus);
  const { track, ring, muted, solo, selected, activity, lastEvent, unmatchedHint } = runtime;
  const color = colorCss(String(track.app.color));
  const chLabel = `CH${track.startChannel + 1}${track.width > 1 ? `–${track.startChannel + track.width}` : ""}`;

  return (
    <article
      className={`track ${dimmed ? "dimmed" : ""} ${activity > 0.2 ? "hot" : ""}`}
      style={{ ["--track" as string]: color }}
    >
      <header className="track-head">
        <button type="button" className="track-title" onClick={() => setFocus(track.key)}>
          <span className="swatch" />
          <span>
            <strong>{track.app.name}</strong>
            <small>
              {chLabel} · MIDI {track.midi.channel}
              {track.midi.cc !== null ? ` CC${track.midi.cc}` : " notes"}
              {track.midi.nrpn ? " NRPN" : ""}
            </small>
          </span>
        </button>
        <div className="track-actions">
          <button
            type="button"
            className={muted ? "on" : ""}
            onClick={() => toggleMute(track.key)}
            title="Mute audio"
          >
            M
          </button>
          <button
            type="button"
            className={solo ? "on solo" : ""}
            onClick={() => toggleSolo(track.key)}
            title="Solo audio + focus view"
          >
            S
          </button>
          <button
            type="button"
            className={selected ? "on cmp" : ""}
            onClick={() => toggleCompare(track.key)}
            title="Toggle compare selection"
          >
            C
          </button>
        </div>
      </header>

      {!compact && (
        <>
          <Scope
            ring={ring}
            color={color}
            dimmed={dimmed || muted}
            label={track.midi.noteMode ? "notes / gates" : "control stream"}
            height={solo || selected ? 140 : 100}
          />
          <WaveProfile ring={ring} color={color} dimmed={dimmed || muted} height={64} />
        </>
      )}

      <footer className="track-meta">
        <span className="pill">{track.midi.noteMode ? "note" : "cc"}</span>
        <span className="pill">{track.midi.usbEnabled ? "usb on" : "usb off"}</span>
        {lastEvent && (
          <span className="pill live">
            {lastEvent.kind}
            {lastEvent.note !== undefined ? ` ${lastEvent.note}` : ""}
            {lastEvent.cc !== undefined ? ` cc${lastEvent.cc}` : ""}
            {lastEvent.value !== undefined ? ` ${(lastEvent.value * 100) | 0}%` : ""}
          </span>
        )}
        {unmatchedHint && <span className="hint">{unmatchedHint}</span>}
      </footer>
    </article>
  );
}
