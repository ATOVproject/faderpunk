import { useEffect, useRef, type ReactNode } from "react";

import { avgOutProfile } from "../audio/sample-ring";
import type { SampleRing } from "../audio/sample-ring";

const COLOR_MAP: Record<string, string> = {
  White: "#ececec",
  Yellow: "#fdc42f",
  Orange: "#c45c26",
  Red: "#e85a4a",
  Lime: "#8bc34a",
  Green: "#3d8b6e",
  Cyan: "#4cafb1",
  SkyBlue: "#5eb8c8",
  Blue: "#4a7ec8",
  Violet: "#ae348b",
  Pink: "#d46aa8",
  PaleGreen: "#8bb89a",
  Sand: "#c4b090",
  Rose: "#d498a8",
  Salmon: "#d4a080",
  LightBlue: "#88b8d0",
  Custom: "#9a9a9a",
};

export function colorCss(tag: string): string {
  if (tag.startsWith("#") || tag.startsWith("rgb")) return tag;
  return COLOR_MAP[tag] ?? "#c8c4bc";
}

interface ScopeProps {
  ring: SampleRing;
  color: string;
  height?: number;
  label?: string;
  /** Inline control at the start of the HUD text row (e.g. monitor note). */
  labelStart?: ReactNode;
  dimmed?: boolean;
  /** Visible history in ms (linear time). Default 8s. */
  windowMs?: number;
  /** Hide the canvas (and let parents collapse) when the window is quiet. */
  collapseWhenQuiet?: boolean;
}

export function Scope({
  ring,
  color,
  height = 120,
  label,
  labelStart,
  dimmed,
  windowMs = 8000,
  collapseWhenQuiet = false,
}: ScopeProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const hudTextRef = useRef<HTMLSpanElement>(null);
  const tmpRef = useRef(new Float32Array(512));

  useEffect(() => {
    const canvas = canvasRef.current;
    const frame = frameRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;
    /** Symmetric hold before toggling collapsed — avoids one-frame flicker both ways. */
    const HOLD_MS = 280;
    let quietSince = 0;
    let activeSince = 0;
    let collapsed = false;

    const draw = () => {
      const bins = Math.min(tmpRef.current.length, Math.max(64, Math.floor(canvas.clientWidth || 64)));
      const view = tmpRef.current.subarray(0, bins);
      const n = ring.resampleWindow(view, windowMs);
      let peak = 0;
      for (let i = 0; i < n; i++) peak = Math.max(peak, view[i] ?? 0);
      const quiet = n < 2 || peak < 0.02;
      const secs = Math.max(1, Math.round(windowMs / 1000));

      if (collapseWhenQuiet) {
        const now = performance.now();
        if (quiet) {
          activeSince = 0;
          if (quietSince === 0) quietSince = now;
          if (!collapsed && now - quietSince >= HOLD_MS) collapsed = true;
        } else {
          quietSince = 0;
          if (activeSince === 0) activeSince = now;
          if (collapsed && now - activeSince >= HOLD_MS) collapsed = false;
        }
        canvas.classList.toggle("is-collapsed", collapsed);
        frame?.classList.toggle("is-collapsed", collapsed);
        if (collapsed) {
          raf = requestAnimationFrame(draw);
          return;
        }
      }

      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (w < 1 || h < 1) {
        raf = requestAnimationFrame(draw);
        return;
      }
      if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
        canvas.width = Math.floor(w * dpr);
        canvas.height = Math.floor(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      ctx.fillStyle = "#101010";
      ctx.fillRect(0, 0, w, h);

      // grid
      ctx.strokeStyle = "rgba(76, 175, 177, 0.12)";
      ctx.lineWidth = 1;
      for (let i = 1; i < 4; i++) {
        const y = (h / 4) * i;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }
      // time ticks (1s)
      ctx.strokeStyle = "rgba(76, 175, 177, 0.08)";
      for (let s = 1; s < secs; s++) {
        const x = (s / secs) * w;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }

      if (n > 1 && !quiet) {
        ctx.beginPath();
        ctx.strokeStyle = color;
        ctx.globalAlpha = dimmed ? 0.35 : 1;
        ctx.lineWidth = 1.5;
        for (let i = 0; i < n; i++) {
          const x = (i / (n - 1)) * w;
          const y = (1 - view[i]) * (h - 4) + 2;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;

        ctx.lineTo(w, h);
        ctx.lineTo(0, h);
        ctx.closePath();
        ctx.fillStyle = color;
        ctx.globalAlpha = dimmed ? 0.05 : 0.12;
        ctx.fill();
        ctx.globalAlpha = 1;
      }

      if (hudTextRef.current && label) {
        hudTextRef.current.textContent = quiet
          ? `${label} · ${secs}s · quiet`
          : `${label} · ${secs}s`;
        hudTextRef.current.classList.toggle("is-quiet", quiet);
      }

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ring, color, dimmed, label, windowMs, collapseWhenQuiet]);

  const aria = [labelStart ? "monitor note" : null, label, "oscilloscope"]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      ref={frameRef}
      className={`scope-frame${collapseWhenQuiet ? " can-collapse" : ""}`}
      style={{ height }}
    >
      {(label || labelStart) && (
        <div className="scope-hud">
          {labelStart}
          {label && <span ref={hudTextRef} className="scope-hud-text" />}
        </div>
      )}
      <canvas ref={canvasRef} className="scope" aria-label={aria} />
    </div>
  );
}

interface ProfileTrace {
  ring: SampleRing;
}

interface ProfileProps {
  /** Out rings — combined into one avg cycle (sum of time-aligned outs). */
  traces: ProfileTrace[];
  color: string;
  height?: number;
  dimmed?: boolean;
}

export function WaveProfile({ traces, color, height = 72, dimmed }: ProfileProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tracesRef = useRef(traces);
  tracesRef.current = traces;
  const ringEpoch = String(traces.length);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || tracesRef.current.length === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;

    const draw = () => {
      const active = tracesRef.current;
      if (active.length === 0) {
        raf = requestAnimationFrame(draw);
        return;
      }
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
        canvas.width = Math.floor(w * dpr);
        canvas.height = Math.floor(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "#101010";
      ctx.fillRect(0, 0, w, h);

      const rings = active.map((t) => t.ring);
      const peak = rings.reduce((p, r) => Math.max(p, r.recentPeak(8000)), 0);
      const quiet = peak < 0.02;
      const profile = avgOutProfile(rings, 96, "add");
      const alpha = dimmed ? 0.35 : 1;

      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.globalAlpha = quiet ? alpha * 0.35 : alpha;
      ctx.lineWidth = 2;
      for (let i = 0; i < profile.length; i++) {
        const x = (i / (profile.length - 1)) * w;
        const y = (1 - profile[i]) * (h - 6) + 3;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;

      ctx.fillStyle = quiet ? "#6a6a6a" : "#9a9a9a";
      ctx.font = "350 10px 'Martian Mono', ui-monospace, monospace";
      // Left edge = mean attack (onset), not the left of the 8s scope window.
      const label =
        rings.length > 1
          ? quiet
            ? "avg pulse · Σ · quiet"
            : "avg pulse · Σ"
          : quiet
            ? "avg pulse · quiet"
            : "avg pulse";
      ctx.fillText(label, 8, 12);

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [color, dimmed, ringEpoch]);

  if (traces.length === 0) return null;
  return <canvas ref={canvasRef} className="scope profile" style={{ height }} />;
}
