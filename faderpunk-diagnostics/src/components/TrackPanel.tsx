import { useEffect, useRef } from "react";
import { colorCss, Scope, WaveProfile } from "./Scope";
import {
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

/** Already covered by wireLabel / card chrome — don’t repeat in the param grid. */
const PARAM_KINDS_IN_SUMMARY = new Set([
  "MidiOut",
  "MidiIn",
  "MidiChannel",
  "MidiCc",
  "MidiNote",
  "MidiMode",
  "MidiNrpn",
  "Color",
]);

function laneLabel(
  role: "in" | "out",
  channel: number,
  outIndex: number,
  outCount: number,
  outName?: string,
): string {
  if (role === "in") return `In · CH${channel}`;
  const short =
    outName
      ?.replace(/^MIDI\s+/i, "")
      .replace(/^Out\s+/i, "Out ")
      .trim() || null;
  if (outCount > 1) {
    const tag = short && !/^Out$/i.test(short) ? short : `Out ${outIndex + 1}`;
    return `${tag} · CH${channel}`;
  }
  return short && !/^Out$/i.test(short) ? `${short} · CH${channel}` : `Out · CH${channel}`;
}

/** Map Out / Out 1 ring activity → 0–1 flash for the fader color block. */
function outFlashLevel(lastT: number, latest: number, now: number): number {
  if (lastT <= 0) return 0;
  const age = now - lastT;
  const hit = Math.max(0.35, latest);
  if (age < 90) return Math.min(1, hit);
  if (age < 420) return hit * (1 - (age - 90) / 330);
  return 0;
}

function padNum(n: number, width: number): string {
  return String(n).padStart(width, "0");
}

/** Fixed-width live event line so noteOn/noteOff don’t shove ch/n/v around. */
function formatLastEvent(ev: {
  kind: string;
  channel?: number;
  cc?: number;
  note?: number;
  velocity?: number;
  rawValue?: number;
}): string {
  const kind =
    ev.kind === "noteOn" ? "noteOn " : ev.kind === "noteOff" ? "noteOff" : ev.kind.padEnd(7, " ");
  const parts = [kind];
  if (ev.channel) parts.push(`ch${padNum(ev.channel, 2)}`);
  if (ev.cc !== undefined) parts.push(`cc${padNum(ev.cc, 3)}`);
  if (ev.note !== undefined) parts.push(`n${padNum(ev.note, 3)}`);
  if (ev.velocity !== undefined) parts.push(`v${padNum(ev.velocity, 3)}`);
  if (
    ev.rawValue !== undefined &&
    ev.kind !== "noteOn" &&
    ev.kind !== "noteOff"
  ) {
    parts.push(`=${ev.rawValue}`);
  }
  return parts.join(" ");
}

export function TrackPanel({ runtime, dimmed, compact }: Props) {
  const toggleMute = useDiag((s) => s.toggleMute);
  const toggleSolo = useDiag((s) => s.toggleSolo);
  const toggleCompare = useDiag((s) => s.toggleCompare);
  const panicTrack = useDiag((s) => s.panicTrack);
  const setFocus = useDiag((s) => s.setFocus);
  const uniqueMidiChannels = useDiag((s) => s.uniqueMidiChannels);
  const setLaneMonitorNote = useDiag((s) => s.setLaneMonitorNote);
  const keyPc = useDiag((s) => s.keyPc);
  const demo = useDiag((s) => s.demo);
  const faderChRef = useRef<HTMLSpanElement>(null);
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
  } = runtime;
  const color = colorCss(String(track.app.color));
  const faderCh =
    track.width > 1
      ? `${track.startChannel + 1}–${track.startChannel + track.width}`
      : String(track.startChannel + 1);
  const groupColor =
    collision && collisionGroup >= 0
      ? GROUP_COLORS[collisionGroup % GROUP_COLORS.length]
      : undefined;
  const cvOnly = !track.hasMidiMirror;
  const outLanes = lanes.filter((l) => l.role === "out");
  const primaryOut = outLanes[0];
  const showMonitorNote = !cvOnly && (track.midi.playCc || !track.midi.noteMode);
  const noteOptions = keyNoteOptions(keyPc);
  const extraParams = (track.paramRows ?? []).filter(
    (row) => !PARAM_KINDS_IN_SUMMARY.has(row.kind),
  );

  useEffect(() => {
    const el = faderChRef.current;
    const ring = primaryOut?.ring;
    if (!el || !ring || cvOnly) {
      el?.style.setProperty("--out-flash", "0");
      return;
    }
    let raf = 0;
    const tick = () => {
      const level = muted || dimmed ? 0 : outFlashLevel(ring.lastT, ring.latest, performance.now());
      el.style.setProperty("--out-flash", level.toFixed(3));
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [primaryOut?.key, primaryOut?.ring, muted, dimmed, cvOnly]);

  return (
    <article
      className={`track ${compact ? "compact" : ""} ${dimmed ? "dimmed" : ""} ${muted ? "is-muted" : ""} ${cvOnly ? "cv-only" : ""} ${activity > 0.2 ? "hot" : ""} ${collision ? "collision" : ""} ${ambiguousHit ? "ambiguous" : ""}`}
      style={
        {
          ["--track"]: color,
          ["--share"]: groupColor ?? "transparent",
        } as Record<string, string>
      }
    >
      {!compact && collision && (
        <div className="share-banner" title="MIDI has no app id — same ch/CC is indistinguishable on the wire">
          <div className="share-copy">
            <strong>Shared on wire</strong>
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

      {!compact && cvOnly && (
        <div className="cv-banner" title="This app has no MidiOut USB mirror — diagnostics can’t scope CV">
          <div className="cv-copy">
            <strong>CV-only · no MIDI mirror</strong>
            <span>Jack/fader I/O only — nothing to scope over USB.</span>
          </div>
          <span className="cv-badge" aria-hidden>
            CV
          </span>
        </div>
      )}

      <header className="track-head">
        <button type="button" className="track-title" onClick={() => setFocus(track.key)}>
          <span
            ref={faderChRef}
            className="fader-ch"
            title={
              cvOnly
                ? `Fader channel ${faderCh} — CV-only (no MIDI scope flash)`
                : `Fader channel ${faderCh} — flashes with Out scope`
            }
            style={{ ["--out-flash" as string]: "0" }}
          >
            {faderCh}
          </span>
          <span className="track-copy">
            <strong>{track.app.name}</strong>
            {!compact && !cvOnly && (
              <small>
                {muted ? "monitor muted · " : ""}
                {wireLabel}
              </small>
            )}
            {!compact && cvOnly && muted && <small className="mute-tag">monitor muted</small>}
            {compact && muted && <small className="mute-tag">muted</small>}
          </span>
        </button>
        <div className="track-actions">
          <button
            type="button"
            className={muted ? "on mute" : ""}
            onClick={() => toggleMute(track.key)}
            title={muted ? "Unmute monitor audio" : "Mute monitor audio"}
            aria-pressed={muted}
            disabled={cvOnly}
          >
            M
          </button>
          <button
            type="button"
            className={solo ? "on solo" : ""}
            onClick={() => toggleSolo(track.key)}
            title="Focus — solo audio + this track only"
          >
            F
          </button>
          <button
            type="button"
            className={selected ? "on cmp" : ""}
            onClick={() => toggleCompare(track.key)}
            title="Toggle compare selection"
          >
            C
          </button>
          <button
            type="button"
            className="panic-slot"
            onClick={() => panicTrack(track.key)}
            title={
              cvOnly
                ? "No MIDI out on this app"
                : `All Notes/Sound Off on MIDI out CH ${track.midi.outChannels.join("/")}`
            }
            disabled={cvOnly}
          >
            P
          </button>
        </div>
      </header>

      {compact && !cvOnly && primaryOut && (
        <div className="mini-scope" title={`Out · CH${primaryOut.channel}`}>
          <Scope
            ring={primaryOut.ring}
            color={color}
            dimmed={dimmed || muted}
            height={28}
            windowMs={4000}
            collapseWhenQuiet
          />
        </div>
      )}

      {compact && cvOnly && (
        <div className="cv-compact" title="No MIDI mirror — scopes unavailable">
          <span>CV-only</span>
          <em>no USB MIDI</em>
        </div>
      )}

      {!compact && (
        <>
          {cvOnly ? (
            <div className="cv-scope-placeholder">
              <span>No MIDI scope</span>
              <em>CV / gate stay on the jack</em>
            </div>
          ) : (
            <>
              {lanes.map((lane) => {
                const outIndex = lane.role === "out" ? outLanes.indexOf(lane) : 0;
                const outName =
                  lane.role === "out" ? track.midi.outChannelNames[outIndex] : undefined;
                const baseLabel =
                  collision && lane.role === "out"
                    ? `shared · ${laneLabel(lane.role, lane.channel, outIndex, outLanes.length, outName)}`
                    : laneLabel(lane.role, lane.channel, outIndex, outLanes.length, outName);
                const noteSelect =
                  showMonitorNote && lane.role === "out" && lane.monitorNote ? (
                    <select
                      className="scope-note-select"
                      title="Monitor pitch for this MIDI out (notes + CC carrier), in the global Key"
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
                  ) : null;
                return (
                  <div key={lane.key} className="scope-block">
                    <Scope
                      ring={lane.ring}
                      color={color}
                      dimmed={dimmed || muted}
                      label={baseLabel}
                      labelStart={noteSelect}
                      height={
                        lane.role === "in"
                          ? 56
                          : solo || selected
                            ? Math.max(88, Math.floor(140 / Math.max(1, outLanes.length)))
                            : Math.max(72, Math.floor(100 / Math.max(1, outLanes.length)))
                      }
                    />
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
            {muted && (
              <span className="pill mute" title="Monitor audio muted — MIDI scopes still update">
                MUTED
              </span>
            )}
            <span className="pill id" title="Layout instance id">
              lid {track.layoutId}
            </span>
            <span className="pill id" title="Firmware app id">
              app {track.app.appId}
            </span>
            {track.app.icon && (
              <span className="pill id" title="App icon">
                {track.app.icon}
              </span>
            )}
            {!cvOnly && (
              <span
                className={`pill ${track.midi.usbEnabled ? "" : "warn"}`}
                title="MidiOut → USB mirror (needed for scopes)"
              >
                {track.midi.usbEnabled ? "usb out" : "usb out off"}
              </span>
            )}
            {!cvOnly && outLanes.length > 1 && (
              <span
                className="pill"
                title={outLanes
                  .map(
                    (l, i) =>
                      `${track.midi.outChannelNames[i] ?? `Out ${i + 1}`} CH${l.channel}`,
                  )
                  .join(" · ")}
              >
                {outLanes.length} outs
              </span>
            )}
            {!cvOnly && lastEvent && (
              <span className="pill live event" title="Last MIDI event on this track">
                {formatLastEvent(lastEvent)}
              </span>
            )}
            {unmatchedHint && <span className="hint">{unmatchedHint}</span>}
          </footer>

          {extraParams.length > 0 && (
            <dl className="param-grid" title="Live AppState — updates on device push + soft poll">
              {extraParams.map((row, i) => (
                <div key={`${i}-${row.kind}-${row.name}`} className="param-row">
                  <dt>{row.name}</dt>
                  <dd key={row.text}>{row.text}</dd>
                </div>
              ))}
            </dl>
          )}

          {track.app.description && (
            <p className="track-desc">{track.app.description}</p>
          )}
        </>
      )}
    </article>
  );
}
