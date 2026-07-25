import { useEffect, useRef } from "react";

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
  return COLOR_MAP[tag] ?? "#c8c4bc";
}

interface ScopeProps {
  ring: SampleRing;
  color: string;
  height?: number;
  label?: string;
  dimmed?: boolean;
  /** Visible history in ms (linear time). Default 8s. */
  windowMs?: number;
}

export function Scope({
  ring,
  color,
  height = 120,
  label,
  dimmed,
  windowMs = 8000,
}: ScopeProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tmpRef = useRef(new Float32Array(512));

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;

    const draw = () => {
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
      const secs = Math.max(1, Math.round(windowMs / 1000));
      ctx.strokeStyle = "rgba(76, 175, 177, 0.08)";
      for (let s = 1; s < secs; s++) {
        const x = (s / secs) * w;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }

      const bins = Math.min(tmpRef.current.length, Math.max(64, Math.floor(w)));
      const view = tmpRef.current.subarray(0, bins);
      const n = ring.resampleWindow(view, windowMs);
      let peak = 0;
      for (let i = 0; i < n; i++) peak = Math.max(peak, view[i] ?? 0);
      const quiet = n > 1 && peak < 0.02;

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

      if (label) {
        ctx.fillStyle = quiet ? "#6a6a6a" : "#9a9a9a";
        ctx.font = "350 11px 'Martian Mono', ui-monospace, monospace";
        ctx.fillText(quiet ? `${label} · ${secs}s · quiet` : `${label} · ${secs}s`, 8, 14);
      }

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ring, color, dimmed, label, windowMs]);

  return (
    <canvas
      ref={canvasRef}
      className="scope"
      style={{ height }}
      aria-label={label ?? "oscilloscope"}
    />
  );
}

interface ProfileProps {
  ring: SampleRing;
  color: string;
  height?: number;
  dimmed?: boolean;
}

export function WaveProfile({ ring, color, height = 72, dimmed }: ProfileProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;

    const draw = () => {
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

      const profile = ring.profile(96);
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.globalAlpha = dimmed ? 0.35 : 1;
      ctx.lineWidth = 2;
      for (let i = 0; i < profile.length; i++) {
        const x = (i / (profile.length - 1)) * w;
        const y = (1 - profile[i]) * (h - 6) + 3;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;

      ctx.fillStyle = "#9a9a9a";
      ctx.font = "350 10px 'Martian Mono', ui-monospace, monospace";
        ctx.fillText("avg cycle", 8, 12);

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ring, color, dimmed]);

  return <canvas ref={canvasRef} className="scope profile" style={{ height }} />;
}
