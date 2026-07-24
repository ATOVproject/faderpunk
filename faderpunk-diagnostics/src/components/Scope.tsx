import { useEffect, useRef } from "react";

import type { SampleRing } from "../audio/sample-ring";

const COLOR_MAP: Record<string, string> = {
  White: "#e8e6e1",
  Yellow: "#e8c84a",
  Orange: "#e8913a",
  Red: "#e85a4a",
  Lime: "#a8d44a",
  Green: "#4ad47a",
  Cyan: "#3ad4c8",
  SkyBlue: "#4aa8e8",
  Blue: "#4a6ae8",
  Violet: "#9a4ae8",
  Pink: "#e84aa8",
  PaleGreen: "#a8d4b8",
  Sand: "#d4c4a0",
  Rose: "#e8a0b0",
  Salmon: "#e8a888",
  LightBlue: "#88c4e8",
  Custom: "#c8c4bc",
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
}

export function Scope({ ring, color, height = 120, label, dimmed }: ScopeProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tmpRef = useRef(new Float32Array(2048));

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

      ctx.fillStyle = "rgba(12, 14, 16, 0.9)";
      ctx.fillRect(0, 0, w, h);

      // grid
      ctx.strokeStyle = "rgba(255,255,255,0.06)";
      ctx.lineWidth = 1;
      for (let i = 1; i < 4; i++) {
        const y = (h / 4) * i;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }

      const n = ring.copyChronological(tmpRef.current);
      if (n > 1) {
        ctx.beginPath();
        ctx.strokeStyle = color;
        ctx.globalAlpha = dimmed ? 0.35 : 1;
        ctx.lineWidth = 1.5;
        for (let i = 0; i < n; i++) {
          const x = (i / (n - 1)) * w;
          const y = (1 - tmpRef.current[i]) * (h - 4) + 2;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;

        // fill under curve
        ctx.lineTo(w, h);
        ctx.lineTo(0, h);
        ctx.closePath();
        ctx.fillStyle = color;
        ctx.globalAlpha = dimmed ? 0.05 : 0.12;
        ctx.fill();
        ctx.globalAlpha = 1;
      }

      if (label) {
        ctx.fillStyle = "rgba(255,255,255,0.45)";
        ctx.font = "11px 'IBM Plex Mono', monospace";
        ctx.fillText(label, 8, 14);
      }

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ring, color, dimmed, label]);

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
      ctx.fillStyle = "rgba(8, 10, 12, 0.95)";
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

      ctx.fillStyle = "rgba(255,255,255,0.4)";
      ctx.font = "10px 'IBM Plex Mono', monospace";
      ctx.fillText("profile", 8, 12);

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ring, color, dimmed]);

  return <canvas ref={canvasRef} className="scope profile" style={{ height }} />;
}
