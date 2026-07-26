import { useEffect } from "react";

import { formatPc } from "../audio/music";
import { useDiag } from "../store";
import { colorCss, Scope } from "./Scope";
import { DevicePanel } from "./DevicePanel";
import { TrackPanel } from "./TrackPanel";

export function AppShell() {
  const status = useDiag((s) => s.status);
  const error = useDiag((s) => s.error);
  const notice = useDiag((s) => s.notice);
  const version = useDiag((s) => s.version);
  const deviceInfo = useDiag((s) => s.deviceInfo);
  const demo = useDiag((s) => s.demo);
  const viewMode = useDiag((s) => s.viewMode);
  const tracks = useDiag((s) => s.tracks);
  const focusKey = useDiag((s) => s.focusKey);
  const masterGain = useDiag((s) => s.masterGain);
  const keyPc = useDiag((s) => s.keyPc);
  const transportRunning = useDiag((s) => s.transportRunning);
  const unmappedLog = useDiag((s) => s.unmappedLog);
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
  const clockBpm = useDiag((s) => s.clockBpm);

  useEffect(() => {
    /** Armed on keydown; one toggle on keyup. Suppresses native button Space→click. */
    let spaceArmed = false;
    /** Ignore transport button click synthesized from the same Space press. */
    let ignoreTransportClickUntil = 0;

    const isTypingTarget = (target: EventTarget | null) => {
      if (!(target instanceof HTMLElement)) return false;
      if (target.isContentEditable) return true;
      const tag = target.tagName;
      if (tag === "TEXTAREA" || tag === "SELECT") return true;
      if (tag !== "INPUT") return false;
      const type = (target as HTMLInputElement).type;
      return (
        type === "text" ||
        type === "search" ||
        type === "password" ||
        type === "email" ||
        type === "number" ||
        type === "url" ||
        type === ""
      );
    };

    const isSpace = (e: KeyboardEvent) => e.code === "Space" || e.key === " ";

    const toggleTransport = () => {
      if (useDiag.getState().transportRunning) transportStop();
      else void transportStart();
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      const target = e.target;
      const st = useDiag.getState().status;

      if (isSpace(e) && !isTypingTarget(target)) {
        // Always prevent Space page/container scroll + button activation.
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.repeat) return;
        // Drop focus so keyup cannot synthesize a second click on Start/Stop.
        const ae = document.activeElement;
        if (ae instanceof HTMLElement && ae.matches("button, a, [role='button']")) {
          ae.blur();
        }
        spaceArmed = st === "ready";
        ignoreTransportClickUntil = performance.now() + 500;
        return;
      }

      if (st === "idle" || st === "error") {
        if (e.key === "Enter" || e.code === "Enter") {
          if (target instanceof HTMLElement && (target.closest("button, a") || isTypingTarget(target))) {
            return;
          }
          e.preventDefault();
          void connect();
        }
        return;
      }

      if (st !== "ready") return;

      if (target instanceof HTMLElement && (target.closest("button, a") || isTypingTarget(target))) {
        return;
      }

      if (e.key === "m" || e.key === "M") {
        e.preventDefault();
        toggleMuteAll();
        return;
      }
      if (e.key === "Escape" || e.key === "p" || e.key === "P") {
        e.preventDefault();
        panic();
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (!isSpace(e)) return;
      if (isTypingTarget(e.target)) {
        spaceArmed = false;
        return;
      }

      e.preventDefault();
      e.stopImmediatePropagation();

      if (useDiag.getState().status !== "ready" || !spaceArmed) {
        spaceArmed = false;
        return;
      }
      spaceArmed = false;
      ignoreTransportClickUntil = performance.now() + 500;
      toggleTransport();
    };

    const onTransportClickCapture = (e: MouseEvent) => {
      if (performance.now() >= ignoreTransportClickUntil) return;
      const el = e.target;
      if (!(el instanceof Element)) return;
      if (!el.closest(".transport-btn")) return;
      e.preventDefault();
      e.stopImmediatePropagation();
    };

    document.addEventListener("keydown", onKeyDown, { capture: true });
    document.addEventListener("keyup", onKeyUp, { capture: true });
    document.addEventListener("click", onTransportClickCapture, { capture: true });
    return () => {
      document.removeEventListener("keydown", onKeyDown, { capture: true });
      document.removeEventListener("keyup", onKeyUp, { capture: true });
      document.removeEventListener("click", onTransportClickCapture, { capture: true });
    };
  }, [connect, toggleMuteAll, panic, transportStart, transportStop]);

  const selectedKeys = new Set(tracks.filter((t) => t.selected).map((t) => t.key));

  const rangePct = (value: number, min: number, max: number) =>
    `${Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100))}%`;

  const visible = tracks.filter((tr) => {
    if (viewMode === "all") return true;
    if (viewMode === "solo") return tr.key === focusKey;
    if (viewMode === "compare") return selectedKeys.has(tr.key) || selectedKeys.size === 0;
    return true;
  });

  return (
    <div
      className={`app${status === "ready" ? " has-side" : ""}${
        status !== "ready" ? " is-start" : ""
      }`}
    >
      <div className="stage">
      <header className="top">
        <div className="brand">
          <img className="brand-logo" src="/img/fp-logo.svg" width="55" height="72" alt="Faderpunk" />
          <div className="brand-text">
            <h1>Scopepunk</h1>
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
              <button
                type="button"
                className="primary"
                onClick={() => void connect()}
                disabled={status === "connecting"}
                title="Connect device (Enter)"
              >
                {status === "connecting" ? "Connecting…" : "Connect device"}
                {status !== "connecting" ? <kbd>Enter</kbd> : null}
              </button>
              <button type="button" onClick={startDemo}>
                Demo mode
              </button>
            </>
          ) : (
            <>
              {!demo && (
                <>
                  <button
                    type="button"
                    className={usbCapable > 0 && usbOn < usbCapable ? "primary" : ""}
                    onClick={() => void enableUsbMidi()}
                    title={
                      usbCapable > 0 && usbOn >= usbCapable
                        ? "All capable apps already have USB MIDI on"
                        : "Enable MidiOut→USB on capable apps"
                    }
                  >
                    Enable USB MIDI
                    {usbCapable > 0 ? ` (${usbOn}/${usbCapable})` : ""}
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

      {status !== "ready" && error && <div className="banner error">{error}</div>}
      {status !== "ready" && notice && <div className="banner notice">{notice}</div>}
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
            <div className="dock-stack">
              {(error || notice) && (
                <div className="dock-status" role="status" aria-live="polite">
                  {error && <div className="banner error">{error}</div>}
                  {notice && <div className="banner notice">{notice}</div>}
                </div>
              )}
              <div className="dock-panel">
              <div className="dock-actions">
                <button
                  type="button"
                  className={`transport-btn ${transportRunning ? "on" : ""}`}
                  onMouseDown={(e) => {
                    // Keep focus off the button so Space never does native activate+our hotkey.
                    e.preventDefault();
                  }}
                  onClick={() => {
                    if (transportRunning) transportStop();
                    else void transportStart();
                  }}
                  tabIndex={-1}
                  title={
                    transportRunning
                      ? "MIDI Stop (Space)"
                      : "Start — MIDI USB clock + host ticks (Space)"
                  }
                  disabled={demo}
                >
                  {transportRunning ? "Stop" : "Start"}
                  <kbd>Space</kbd>
                </button>

                <button
                  type="button"
                  className={`listen-btn ${allMuted ? "" : "on"}`}
                  onClick={toggleMuteAll}
                  title={
                    allMuted
                      ? "Unmute all tracks (M)"
                      : "Mute all tracks (M)"
                  }
                  disabled={tracks.length === 0}
                  aria-pressed={allMuted}
                >
                  Mute
                  <kbd>M</kbd>
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

              <div className="toolbar-meters">
                <label
                  className="slider meter-slider meter-bpm"
                  title="Host MIDI clock tempo — written to device Internal BPM"
                >
                  <span className="meter-label">BPM</span>
                  <input
                    type="range"
                    min={40}
                    max={240}
                    step={1}
                    value={Math.round(clockBpm)}
                    onChange={(e) => setClockBpm(Number(e.target.value))}
                    style={{ ["--pct" as string]: rangePct(Math.round(clockBpm), 40, 240) }}
                  />
                  <em className="meter-val">{Math.round(clockBpm)}</em>
                </label>

                <label
                  className="slider meter-slider meter-key"
                  title="Musical key for CC / hybrid monitor carriers"
                >
                  <span className="meter-label">Key</span>
                  <input
                    type="range"
                    min={0}
                    max={11}
                    step={1}
                    value={keyPc}
                    onChange={(e) => setKeyPc(Number(e.target.value))}
                    style={{ ["--pct" as string]: rangePct(keyPc, 0, 11) }}
                  />
                  <em className="meter-val">{formatPc(keyPc)}</em>
                </label>

                <label
                  className="slider meter-slider meter-vol"
                  title="Monitor master volume"
                >
                  <span className="meter-label">Vol</span>
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={masterGain}
                    onChange={(e) => setMasterGain(Number(e.target.value))}
                    style={{ ["--pct" as string]: rangePct(masterGain, 0, 1) }}
                  />
                  <em className="meter-val">{Math.round(masterGain * 100)}</em>
                </label>
              </div>

              <div className="seg dock-seg" role="group" aria-label="View mode">
                <button
                  type="button"
                  className={viewMode === "all" ? "on" : ""}
                  aria-pressed={viewMode === "all"}
                  onClick={() => setViewMode("all")}
                >
                  All
                </button>
                <button
                  type="button"
                  className={viewMode === "solo" ? "on" : ""}
                  aria-pressed={viewMode === "solo"}
                  onClick={() => setViewMode("solo")}
                >
                  Focus
                </button>
                <button
                  type="button"
                  className={viewMode === "compare" ? "on" : ""}
                  aria-pressed={viewMode === "compare"}
                  onClick={() => setViewMode("compare")}
                >
                  Compare
                </button>
              </div>
              </div>
            </div>
          </footer>
        </>
      )}

      {status === "idle" && (
        <section className="hero-help">
          <ol>
            <li>
              Press <strong>Enter</strong> or <strong>Connect device</strong>, then{" "}
              <strong>Enable USB MIDI</strong>. Host always echoes USB-Out → USB-In so MidiIn
              apps hear other apps.
            </li>
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
          <h2>Layout</h2>
          <ul className="track-list">
            {tracks.map((tr) => (
              <li key={tr.key}>
                <TrackPanel runtime={tr} compact />
              </li>
            ))}
          </ul>

          <DevicePanel
            info={deviceInfo}
            live={{
              usbOn,
              usbCapable,
              transportRunning,
              monitorOn: !allMuted,
            }}
          />

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
