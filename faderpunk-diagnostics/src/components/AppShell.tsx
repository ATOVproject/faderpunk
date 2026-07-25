import { useEffect } from "react";

import { formatPc } from "../audio/music";
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
  const keyPc = useDiag((s) => s.keyPc);
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
  const setKeyPc = useDiag((s) => s.setKeyPc);
  const setClockBpm = useDiag((s) => s.setClockBpm);
  const allMuted = tracks.length > 0 && tracks.every((tr) => tr.muted);
  const toggleMuteAll = useDiag((s) => s.toggleMuteAll);
  const panic = useDiag((s) => s.panic);
  const transportStart = useDiag((s) => s.transportStart);
  const transportStop = useDiag((s) => s.transportStop);
  const refreshParams = useDiag((s) => s.refreshParams);
  const enableUsbMidi = useDiag((s) => s.enableUsbMidi);
  const uniqueMidiChannels = useDiag((s) => s.uniqueMidiChannels);
  const collisions = useDiag((s) => s.collisions);
  const collisionsBannerDismissed = useDiag((s) => s.collisionsBannerDismissed);
  const dismissCollisionsBanner = useDiag((s) => s.dismissCollisionsBanner);
  const loopbackCount = useDiag((s) => s.loopbackCount);
  const clockSrc = useDiag((s) => s.clockSrc);
  const clockBpm = useDiag((s) => s.clockBpm);

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
        else void transportStart();
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
    <div className={`app${status === "ready" ? " has-side" : ""}`}>
      <div className="stage">
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
                    Unique MIDI
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
      {status === "ready" && collisions.length > 0 && !collisionsBannerDismissed && (
        <div className="banner share">
          <div className="banner-share-head">
            <strong>Shared MIDI on the wire</strong>
            <button
              type="button"
              className="banner-dismiss"
              onClick={dismissCollisionsBanner}
              title="Dismiss"
              aria-label="Dismiss shared MIDI warning"
            >
              ×
            </button>
          </div>
          <ul>
            {collisions.map((c) => (
              <li key={c.key}>
                <code>{c.key.replace(/:/g, " · ")}</code> — {c.label} (indistinguishable)
              </li>
            ))}
          </ul>
          {!demo && (
            <button type="button" className="primary" onClick={() => void uniqueMidiChannels()}>
              Unique MIDI
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

          {portSummary && (
            <p className="ports">ports · {portSummary}</p>
          )}

          <p className="legend">
            <strong>Enter</strong> host Start/Stop (MIDI USB clock) · <strong>Space</strong> mute all /
            unmute all · <strong>Esc</strong> panic · After Editor push: <strong>Reconnect</strong>{" "}
            (layout) or Refresh (params). Host echoes USB-Out → USB-In. Flat scopes with clocks
            ticking → enable <strong>MidiOut→USB</strong> on apps.
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
              <div className="empty">
                {tracks.length === 0
                  ? "No apps in the layout — push a setup from the Editor, or Reconnect after changing the device layout."
                  : "No tracks in this view. Select apps with C or switch to All."}
              </div>
            )}
          </main>

          <footer className="dock">
            <div className="dock-panel">
              <div className="dock-actions">
                <button
                  type="button"
                  className={`transport-btn ${transportRunning ? "on" : ""}`}
                  onClick={() => {
                    if (transportRunning) transportStop();
                    else void transportStart();
                  }}
                  title={
                    transportRunning
                      ? "MIDI Stop (Enter)"
                      : "Start — MIDI USB clock + host ticks (Enter)"
                  }
                  disabled={demo}
                >
                  {transportRunning ? "Stop" : "Start"}
                  <kbd>Enter</kbd>
                </button>

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
              </div>

              <div className="stats dock-stats">
                <div className="stats-line">
                  <span>usb {usbOn}/{usbCapable}</span>
                  <span title="Device clock source">{clockSrc ?? "clk?"}</span>
                  <span>{ccCount} cc</span>
                  <span>{noteCount} notes</span>
                </div>
                <div className="stats-line">
                  <span>{clockCount} clocks</span>
                  <span className="live">echo {loopbackCount}</span>
                  <span className={transportRunning ? "live" : ""}>
                    {transportRunning ? "running" : "stopped"}
                  </span>
                  <span className={allMuted ? "" : "live"}>
                    {allMuted ? "all muted" : "monitor on"}
                  </span>
                </div>
              </div>

              <div className="toolbar-meters">
                <label
                  className={`slider bpm-slider ${transportRunning ? "live" : ""}`}
                  title="Host MIDI clock tempo — written to device Internal BPM"
                >
                  <input
                    type="range"
                    min={40}
                    max={240}
                    step={1}
                    value={Math.round(clockBpm)}
                    onChange={(e) => setClockBpm(Number(e.target.value))}
                  />
                  <span
                    className={`bpm-readout ${transportRunning ? "live" : ""}`}
                  >
                    {Math.round(clockBpm)}
                    <small>BPM</small>
                  </span>
                </label>

                <label
                  className="slider meter-slider"
                  title="Musical key for CC / hybrid monitor carriers. Each scope picks a scale degree."
                >
                  <span className="meter-label">Key</span>
                  <input
                    type="range"
                    min={0}
                    max={11}
                    step={1}
                    value={keyPc}
                    onChange={(e) => setKeyPc(Number(e.target.value))}
                  />
                  <em className="meter-val">{formatPc(keyPc)}</em>
                </label>

                <label className="slider meter-slider" title="Monitor master volume">
                  <span className="meter-label">Vol</span>
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={masterGain}
                    onChange={(e) => setMasterGain(Number(e.target.value))}
                  />
                  <em className="meter-val">{Math.round(masterGain * 100)}</em>
                </label>
              </div>

              <div className="seg dock-seg">
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
            </div>
          </footer>
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

      {status === "ready" && (
        <aside className="side" aria-label="Layout slots">
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
                      ch{ev.channel}
                      {ev.kind === "cc" || ev.kind === "nrpn"
                        ? ` ${ev.kind}${ev.cc !== undefined ? ev.cc : ""}`
                        : ` ${ev.kind}`}
                      {ev.note !== undefined ? ` n${ev.note}` : ""}
                    </li>
                  ))}
              </ul>
            </>
          )}
        </aside>
      )}
    </div>
  );
}
