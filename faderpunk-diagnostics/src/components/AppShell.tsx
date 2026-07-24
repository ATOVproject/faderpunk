import { useDiag } from "../store";
import { TrackPanel } from "./TrackPanel";

export function AppShell() {
  const status = useDiag((s) => s.status);
  const error = useDiag((s) => s.error);
  const version = useDiag((s) => s.version);
  const demo = useDiag((s) => s.demo);
  const viewMode = useDiag((s) => s.viewMode);
  const tracks = useDiag((s) => s.tracks);
  const focusKey = useDiag((s) => s.focusKey);
  const masterGain = useDiag((s) => s.masterGain);
  const waveRate = useDiag((s) => s.waveRate);
  const unmappedLog = useDiag((s) => s.unmappedLog);
  const clockCount = useDiag((s) => s.clockCount);
  const connect = useDiag((s) => s.connect);
  const disconnect = useDiag((s) => s.disconnect);
  const startDemo = useDiag((s) => s.startDemo);
  const setViewMode = useDiag((s) => s.setViewMode);
  const setMasterGain = useDiag((s) => s.setMasterGain);
  const setWaveRate = useDiag((s) => s.setWaveRate);
  const refreshParams = useDiag((s) => s.refreshParams);

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
          <h1>Faderpunk Diagnostics</h1>
          <p>
            Live MIDI scope · waveform profile · audible monitor
            {version ? ` · fw ${version}` : ""}
            {demo ? " · demo" : ""}
          </p>
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
                <button type="button" onClick={() => void refreshParams()}>
                  Refresh params
                </button>
              )}
              <button type="button" onClick={disconnect}>
                Disconnect
              </button>
            </>
          )}
        </div>
      </header>

      {error && <div className="banner error">{error}</div>}

      {status === "ready" && (
        <>
          <section className="toolbar">
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
            </label>

            <label className="slider">
              <span>Wave Hz</span>
              <input
                type="range"
                min={1}
                max={30}
                step={0.5}
                value={waveRate}
                onChange={(e) => setWaveRate(Number(e.target.value))}
              />
            </label>

            <div className="stats">
              <span>{tracks.length} apps</span>
              <span>{clockCount} clocks</span>
            </div>
          </section>

          <p className="legend">
            <strong>M</strong> mute audio · <strong>S</strong> solo (hear + focus) ·{" "}
            <strong>C</strong> mark for compare · combined output is the unmuted mix.
            Values are MIDI mirrors of CV (7-bit CC / notes), not raw jack voltage.
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
            <li>Enable <strong>MidiOut → USB</strong> on each app you want to monitor (Configurator).</li>
            <li>Use Chromium with SysEx permission (or <code>npm run chrome</code>).</li>
            <li>Connect — config cable loads layout; performance cable feeds the scopes.</li>
          </ol>
          <p className="note">
            CV-only apps (Quantizer, Slew, Follower, Offset, AD, MIDI→CV) have no USB mirror and
            cannot be visualized without a firmware telemetry feature.
          </p>
        </section>
      )}
    </div>
  );
}
