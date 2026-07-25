import { colorCss, Scope, WaveProfile } from "./Scope";
import {
  formatMonitorNote,
  keyNoteOptions,
  monitorNoteValue,
  parseMonitorNoteValue,
} from "../audio/music";
import type { TrackRuntime } from "../store";
import { useDiag } from "../store";

interface Props {
  runtime: TrackRuntime;
  dimmed?: boolean;
  compact?: boolean;
}

const GROUP_COLORS = ["#fdc42f", "#ae348b", "#4cafb1", "#c45c26", "#e85a4a"];

function laneLabel(
  role: "in" | "out",
  channel: number,
  outIndex: number,
  outCount: number,
): string {
  if (role === "in") return `In · CH${channel}`;
  if (outCount > 1) return `Out ${outIndex + 1} · CH${channel}`;
  return `Out · CH${channel}`;
}

export function TrackPanel({ runtime, dimmed, compact }: Props) {
  const toggleMute = useDiag((s) => s.toggleMute);
  const toggleSolo = useDiag((s) => s.toggleSolo);
  const toggleCompare = useDiag((s) => s.toggleCompare);
  const setFocus = useDiag((s) => s.setFocus);
  const uniqueMidiChannels = useDiag((s) => s.uniqueMidiChannels);
  const setLaneMonitorNote = useDiag((s) => s.setLaneMonitorNote);
  const keyPc = useDiag((s) => s.keyPc);
  const demo = useDiag((s) => s.demo);
  const {
    track,
    lanes,
    muted,
    solo,
    selected,
    activity,
    lastEvent,
    unmatchedHint,
    collision,
    wireLabel,
    collisionPeers,
    collisionGroup,
    ambiguousHit,
    inputLevel,
  } = runtime;
  const color = colorCss(String(track.app.color));
  const chLabel = `CH${track.startChannel + 1}${track.width > 1 ? `–${track.startChannel + track.width}` : ""}`;
  const groupColor =
    collision && collisionGroup >= 0
      ? GROUP_COLORS[collisionGroup % GROUP_COLORS.length]
      : undefined;
  const hasMidiIn = track.midi.inChannel !== null;
  const outLanes = lanes.filter((l) => l.role === "out");
  const primaryOut = outLanes[0];
  const showMonitorNote = track.midi.playCc || !track.midi.noteMode;
  const noteOptions = keyNoteOptions(keyPc);

  return (
    <article
      className={`track ${dimmed ? "dimmed" : ""} ${activity > 0.2 ? "hot" : ""} ${collision ? "collision" : ""} ${ambiguousHit ? "ambiguous" : ""}`}
      style={
        {
          ["--track"]: color,
          ["--share"]: groupColor ?? "transparent",
          ["--in-level"]: String(inputLevel),
        } as Record<string, string>
      }
    >
      {collision && (
        <div className="share-banner" title="MIDI has no app id — same ch/CC is indistinguishable on the wire">
          <div className="share-copy">
            <strong>Shared on wire · {wireLabel}</strong>
            <span>
              Same stream as {collisionPeers.join(", ")} — scopes/audio can’t tell who sent what.
            </span>
          </div>
          {!demo && (
            <button type="button" className="share-fix" onClick={() => void uniqueMidiChannels()}>
              Split
            </button>
          )}
        </div>
      )}

      <header className="track-head">
        <button type="button" className="track-title" onClick={() => setFocus(track.key)}>
          <span className="swatch" />
          <span>
            <strong>{track.app.name}</strong>
            <small>
              {chLabel} · {wireLabel}
            </small>
          </span>
        </button>
        <div className="track-actions">
          {hasMidiIn && (
            <span
              className={`in-led ${inputLevel > 0.08 ? "lit" : ""}`}
              title={`In CH ${track.midi.inChannel}${track.midi.inUsb ? " USB" : ""} — host echoes USB-Out into device MidiIn`}
              aria-label="MIDI input activity"
            >
              <span className="in-led-core" />
              IN
            </span>
          )}
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
          {lanes.map((lane) => {
            const outIndex = lane.role === "out" ? outLanes.indexOf(lane) : 0;
            return (
              <div key={lane.key} className="scope-block">
                <Scope
                  ring={lane.ring}
                  color={color}
                  dimmed={dimmed || muted}
                  label={
                    collision && lane.role === "out"
                      ? `shared · ${laneLabel(lane.role, lane.channel, outIndex, outLanes.length)}`
                      : laneLabel(lane.role, lane.channel, outIndex, outLanes.length)
                  }
                  height={
                    lane.role === "in"
                      ? 56
                      : solo || selected
                        ? Math.max(88, Math.floor(140 / Math.max(1, outLanes.length)))
                        : Math.max(72, Math.floor(100 / Math.max(1, outLanes.length)))
                  }
                />
                {showMonitorNote && lane.role === "out" && lane.monitorNote && (
                  <label
                    className="scope-note"
                    title="CC carrier for this MIDI out — scale degree in the global Key"
                  >
                    <span>Note</span>
                    <select
                      value={monitorNoteValue(lane.monitorNote)}
                      onChange={(e) =>
                        setLaneMonitorNote(
                          track.key,
                          lane.key,
                          parseMonitorNoteValue(e.target.value),
                        )
                      }
                    >
                      {noteOptions.map((opt) => (
                        <option key={opt.value} value={opt.value}>
                          {opt.label}
                        </option>
                      ))}
                    </select>
                    <em>{formatMonitorNote(keyPc, lane.monitorNote)}</em>
                  </label>
                )}
                {!showMonitorNote && lane.role === "out" && (
                  <div
                    className="scope-note"
                    title="This app sends MIDI notes — pitch comes from the wire (no CC carrier)"
                  >
                    <span>Pitch</span>
                    <em>
                      {track.midi.setupNotes.length > 0
                        ? `n${track.midi.setupNotes.join("/")} · wire`
                        : "from wire notes"}
                    </em>
                  </div>
                )}
              </div>
            );
          })}
          {primaryOut && (
            <WaveProfile
              ring={primaryOut.ring}
              color={color}
              dimmed={dimmed || muted}
              height={64}
            />
          )}
        </>
      )}

      <footer className="track-meta">
        <span className="pill">
          {track.midi.noteMode && track.midi.playCc
            ? "note+cc"
            : track.midi.noteMode
              ? "note"
              : "cc"}
        </span>
        <span className="pill">{track.midi.usbEnabled ? "usb out" : "usb out off"}</span>
        {hasMidiIn && (
          <span className={`pill ${track.midi.inUsb ? "" : "warn"}`}>
            {track.midi.inUsb ? "usb in" : "usb in off"}
          </span>
        )}
        {outLanes.length > 1 && <span className="pill">{outLanes.length} outs</span>}
        {lastEvent && (
          <span className="pill live">
            {lastEvent.kind}
            {lastEvent.cc !== undefined ? ` ${lastEvent.cc}` : ""}
            {lastEvent.note !== undefined ? ` n${lastEvent.note}` : ""}
          </span>
        )}
        {unmatchedHint && <span className="hint">{unmatchedHint}</span>}
      </footer>
    </article>
  );
}
