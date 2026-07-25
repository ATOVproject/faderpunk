import { useEffect } from "react";

import { waveRateToCcPitchHz } from "../audio/engine";
import { useDiag } from "../store";
import { colorCss, Scope } from "./Scope";
import { TrackPanel } from "./TrackPanel";

export function AppShell() {
  const status = useDiag((s) => s.status);
  const error = useDiag((s) => s.error);
  const notice = useDiag((s) => s.notice);
  const version = useDiag((s) => s.version);
  const demo = useDiag((s) => s.demo);
  const viewMode = useDiag((s) => s.viewMode);
  const tracks = useDiag((s) => s.tracks);
  const focusKey = useDiag((s) => s.focusKey);
  const masterGain = useDiag((s) => s.masterGain);
  const waveRate = useDiag((s) => s.waveRate);
  const transportRunning = useDiag((s) => s.transportRunning);
  const unmappedLog = useDiag((s) => s.unmappedLog);
  const clockCount = useDiag((s) => s.clockCount);
  const ccCount = useDiag((s) => s.ccCount);
  const noteCount = useDiag((s) => s.noteCount);
  const portSummary = useDiag((s) => s.portSummary);
  const usbOn = useDiag((s) => s.usbOn);
  const usbCapable = useDiag((s) => s.usbCapable);
  const busRing = useDiag((s) => s.busRing);
  const connect = useDiag((s) => s.connect);
  const disconnect = useDiag((s) => s.disconnect);
  const startDemo = useDiag((s) => s.startDemo);
  const setViewMode = useDiag((s) => s.setViewMode);
  const setMasterGain = useDiag((s) => s.setMasterGain);
  const setWaveRate = useDiag((s) => s.setWaveRate);
  const allMuted = tracks.length > 0 && tracks.every((tr) => tr.muted);
  const toggleMuteAll = useDiag((s) => s.toggleMuteAll);
  const panic = useDiag((s) => s.panic);
  const transportStart = useDiag((s) => s.transportStart);
  const transportStop = useDiag((s) => s.transportStop);
  const refreshParams = useDiag((s) => s.refreshParams);
  const enableUsbMidi = useDiag((s) => s.enableUsbMidi);
  const uniqueMidiChannels = useDiag((s) => s.uniqueMidiChannels);
  const collisions = useDiag((s) => s.collisions);
  const loopbackCount = useDiag((s) => s.loopbackCount);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      const typing =
        tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable;
      if (useDiag.getState().status !== "ready") return;

      if (!typing && (e.code === "Space" || e.key === " ")) {
        e.preventDefault();
        toggleMuteAll();
        return;
      }
      if (!typing && (e.key === "Escape" || e.key === "p" || e.key === "P")) {
        e.preventDefault();
        panic();
        return;
      }
      if (!typing && (e.key === "Enter" || e.code === "Enter")) {
        e.preventDefault();
        if (useDiag.getState().transportRunning) transportStop();
        else transportStart();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleMuteAll, panic, transportStart, transportStop]);

  const selectedKeys = new Set(tracks.filter((t) => t.selected).map((t) => t.key));

  const visible = tracks.filter((tr) => {
    if (viewMode === "all") return true;
    if (viewMode === "solo") return tr.key === focusKey;
    if (viewMode === "compare") return selectedKeys.has(tr.key) || selectedKeys.size === 0;
    return true;
  });

  return (
    <div className="app">
      <header className="top">
        <div className="brand">
          <img className="brand-logo" src="/img/fp-logo.svg" width="55" height="72" alt="Faderpunk" />
          <div className="brand-text">
            <h1>Diagnostics</h1>
            <p className="compat-note">
              {version ? <span className="app-ver">fw {version}</span> : null}
              Live MIDI scope · waveform profile · audible monitor
              {demo ? " · demo" : ""}
            </p>
          </div>
        </div>
        <div className="top-actions">
          {status !== "ready" ? (
            <>
              <button type="button" className="primary" onClick={() => void connect()} disabled={status === "connecting"}>
                {status === "connecting" ? "Connecting…" : "Connect device"}
              </button>
              <button type="button" onClick={startDemo}>
                Demo mode
              </button>
            </>
          ) : (
            <>
              {!demo && (
                <>
                  <button type="button" className="primary" onClick={() => void enableUsbMidi()}>
                    Enable USB MIDI
                  </button>
                  <button
                    type="button"
                    className={collisions.length > 0 ? "primary" : ""}
                    onClick={() => void uniqueMidiChannels()}
                    title="Assign MIDI channels 1…N so apps can be told apart on the wire"
                  >
                    Unique MIDI channels
                    {collisions.length > 0 ? ` (${collisions.length})` : ""}
                  </button>
                  <button type="button" onClick={() => void refreshParams()}>
                    Refresh params
                  </button>
                </>
              )}
              <button type="button" onClick={disconnect}>
                Disconnect
              </button>
            </>
          )}
        </div>
      </header>

      {error && <div className="banner error">{error}</div>}
      {notice && <div className="banner notice">{notice}</div>}
      {status === "ready" && collisions.length > 0 && (
        <div className="banner share">
          <strong>Shared MIDI on the wire</strong>
          <ul>
            {collisions.map((c) => (
              <li key={c.key}>
                <code>{c.key.replace(/:/g, " · ")}</code> — {c.label} (indistinguishable)
              </li>
            ))}
          </ul>
          {!demo && (
            <button type="button" className="primary" onClick={() => void uniqueMidiChannels()}>
              Unique MIDI channels
            </button>
          )}
          <span className="banner-hint">
            Close the Configurator first — both use the same SysEx cable.
          </span>
        </div>
      )}

      {status === "ready" && (
        <>
          <section className="bus">
            <Scope ring={busRing} color={colorCss("Cyan")} height={72} label="USB MIDI bus (all CC / notes)" />
          </section>

          <section className="toolbar">
            <div className="seg transport">
              <button
                type="button"
                className={transportRunning ? "on" : ""}
                onClick={transportStart}
                title="MIDI Start — device follows if Clock Src = MIDI USB"
                disabled={demo}
              >
                Start
              </button>
              <button
                type="button"
                className={!transportRunning ? "on stop" : ""}
                onClick={transportStop}
                title="MIDI Stop"
                disabled={demo}
              >
                Stop
              </button>
              <kbd className="seg-kbd">Enter</kbd>
            </div>

            <button
              type="button"
              className={`listen-btn ${allMuted ? "" : "on"}`}
              onClick={toggleMuteAll}
              title="Mute or unmute every track (Space)"
              disabled={tracks.length === 0}
            >
              {allMuted ? "Unmute all" : "Mute all"}
              <kbd>Space</kbd>
            </button>

            <button
              type="button"
              className="panic-btn"
              onClick={panic}
              title="Escape / P — MIDI Stop, silence monitor, All Notes Off"
            >
              Panic
              <kbd>Esc</kbd>
            </button>

            <div className="seg">
              <button
                type="button"
                className={viewMode === "all" ? "on" : ""}
                onClick={() => setViewMode("all")}
              >
                All
              </button>
              <button
                type="button"
                className={viewMode === "solo" ? "on" : ""}
                onClick={() => setViewMode("solo")}
              >
                Focus
              </button>
              <button
                type="button"
                className={viewMode === "compare" ? "on" : ""}
                onClick={() => setViewMode("compare")}
              >
                Compare
              </button>
            </div>

            <label className="slider">
              <span>Master</span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={masterGain}
                onChange={(e) => setMasterGain(Number(e.target.value))}
              />
              <em>{Math.round(masterGain * 100)}</em>
            </label>

            <label
              className="slider"
              title="Pitch for all CC monitors — amplitude follows the envelope (ducks). Notes stay MIDI pitch."
            >
              <span>CC Hz</span>
              <input
                type="range"
                min={1}
                max={30}
                step={0.5}
                value={waveRate}
                onChange={(e) => setWaveRate(Number(e.target.value))}
              />
              <em>{Math.round(waveRateToCcPitchHz(waveRate))} Hz</em>
            </label>

            <div className="stats">
              <span>usb {usbOn}/{usbCapable}</span>
              <span>{ccCount} cc</span>
              <span>{noteCount} notes</span>
              <span>{clockCount} clocks</span>
              <span className="live">echo {loopbackCount}</span>
              <span className={transportRunning ? "live" : ""}>
                {transportRunning ? "running" : "stopped"}
              </span>
              <span className={allMuted ? "" : "live"}>{allMuted ? "all muted" : "monitor on"}</span>
            </div>
          </section>

          {portSummary && (
            <p className="ports">ports · {portSummary}</p>
          )}

          <p className="legend">
            <strong>Enter</strong> MIDI Start/Stop · <strong>Space</strong> mute all / unmute all ·{" "}
            <strong>Esc</strong> panic · Host always echoes USB-Out → USB-In (no on-device USB
            loop). Transport needs Clock Src = MIDI USB. If the bus scope is flat but clocks tick,
            apps need <strong>MidiOut→USB</strong>.
          </p>

          <main className={`grid mode-${viewMode}`}>
            {visible.map((tr) => (
              <TrackPanel
                key={tr.key}
                runtime={tr}
                dimmed={viewMode === "compare" && selectedKeys.size > 0 && !tr.selected}
              />
            ))}
            {visible.length === 0 && (
              <div className="empty">No tracks in this view. Select apps with C or switch to All.</div>
            )}
          </main>

          <aside className="side">
            <h2>Layout</h2>
            <ul className="track-list">
              {tracks.map((tr) => (
                <li key={tr.key}>
                  <TrackPanel runtime={tr} compact />
                </li>
              ))}
            </ul>

            {unmappedLog.length > 0 && (
              <>
                <h2>Unmapped MIDI</h2>
                <ul className="log">
                  {unmappedLog
                    .slice()
                    .reverse()
                    .slice(0, 12)
                    .map((ev, i) => (
                      <li key={`${ev.t}-${i}`}>
                        ch{ev.channel} {ev.kind}
                        {ev.cc !== undefined ? ` cc${ev.cc}` : ""}
                        {ev.note !== undefined ? ` n${ev.note}` : ""}
                      </li>
                    ))}
                </ul>
              </>
            )}
          </aside>
        </>
      )}

      {status === "idle" && (
        <section className="hero-help">
          <ol>
            <li>Connect, then <strong>Enable USB MIDI</strong>. Host always echoes USB-Out → USB-In so MidiIn apps hear other apps.</li>
            <li>Use Chromium with SysEx permission (or <code>pnpm chrome</code>).</li>
            <li>The top <strong>USB MIDI bus</strong> scope shows any CC/notes before per-app routing.</li>
          </ol>
          <p className="note">
            Clock alone means the performance port works; flat waves mean apps are not mirroring to USB yet.
          </p>
        </section>
      )}

      <a
        className="maker"
        href="https://kosmar.design/"
        target="_blank"
        rel="noreferrer"
        title="kosmar.design"
        aria-label="kosmar.design"
      >
        <img className="maker__logo" src="/img/kosmar.svg" alt="kosmar" />
      </a>
    </div>
  );
}
