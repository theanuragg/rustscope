"use client";

import React, {
  useRef,
  useEffect,
  useCallback,
  useMemo,
  useState,
} from "react";
import type { ProfileFrame, ProfilerState, LayoutFrame } from "@/types/profiler";
import { computeLayout, truncateLabel } from "@/lib/layout";
import { frameColor } from "@/lib/colors";

interface Props {
  frames: ProfileFrame[];
  state: ProfilerState;
  onZoomIn: (frame: ProfileFrame) => void;
  onHover: (frame: ProfileFrame | null) => void;
  width: number;
}

const ROW_HEIGHT = 22;
const GAP = 1;
const FONT = "11px 'JetBrains Mono', monospace";

export function FlameCanvas({ frames, state, onZoomIn, onHover, width }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [tooltipFrame, setTooltipFrame] = useState<{
    frame: LayoutFrame;
    x: number;
    y: number;
  } | null>(null);

  // Compute layout — expensive, memoised
  const { layout, totalHeight } = useMemo(
    () =>
      computeLayout(frames, {
        width,
        rowHeight: ROW_HEIGHT,
        gap: GAP,
        state,
      }),
    [frames, state, width]
  );

  // Draw to canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = totalHeight * dpr;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${totalHeight}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    // Background
    ctx.fillStyle = "#fafaf8";
    ctx.fillRect(0, 0, width, totalHeight);

    ctx.font = FONT;
    ctx.textBaseline = "middle";

    for (const f of layout) {
      // Frame rect
      ctx.fillStyle = f.color;
      ctx.beginPath();
      roundRect(ctx, f.px + GAP, f.py, Math.max(f.pw - GAP * 2, 0.5), ROW_HEIGHT, 2);
      ctx.fill();

      // Label
      if (f.pw > 20) {
        const label = truncateLabel(f.displayName, f.pw - 8);
        if (label) {
          ctx.fillStyle = f.textColor;
          ctx.fillText(label, f.px + GAP + 4, f.py + ROW_HEIGHT / 2 + 0.5);
        }
      }
    }
  }, [layout, totalHeight, width]);

  // Hit test — find frame under cursor
  const hitTest = useCallback(
    (cx: number, cy: number): LayoutFrame | null => {
      // Search in reverse (topmost drawn = last)
      for (let i = layout.length - 1; i >= 0; i--) {
        const f = layout[i];
        if (cx >= f.px && cx <= f.px + f.pw && cy >= f.py && cy <= f.py + ROW_HEIGHT) {
          return f;
        }
      }
      return null;
    },
    [layout]
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const rect = canvasRef.current!.getBoundingClientRect();
      const cx = e.clientX - rect.left;
      const cy = e.clientY - rect.top;
      const hit = hitTest(cx, cy);
      onHover(hit);
      setTooltipFrame(hit ? { frame: hit, x: cx, y: cy } : null);
    },
    [hitTest, onHover]
  );

  const handleMouseLeave = useCallback(() => {
    onHover(null);
    setTooltipFrame(null);
  }, [onHover]);

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const rect = canvasRef.current!.getBoundingClientRect();
      const hit = hitTest(e.clientX - rect.left, e.clientY - rect.top);
      if (hit) onZoomIn(hit);
    },
    [hitTest, onZoomIn]
  );

  return (
    <div style={{ position: "relative", width, height: totalHeight }}>
      <canvas
        ref={canvasRef}
        style={{ cursor: "pointer", display: "block" }}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
        onClick={handleClick}
      />
      {tooltipFrame && (
        <FrameTooltip
          frame={tooltipFrame.frame}
          x={tooltipFrame.x}
          y={tooltipFrame.y}
          containerWidth={width}
        />
      )}
    </div>
  );
}

// ─── Tooltip ────────────────────────────────────────────────────────────────

import { formatNs, formatBytes } from "@/lib/demangle";

function FrameTooltip({
  frame,
  x,
  y,
  containerWidth,
}: {
  frame: LayoutFrame;
  x: number;
  y: number;
  containerWidth: number;
}) {
  const hotness =
    frame.totalPct >= 20
      ? { label: "major bottleneck", cls: "text-red-600" }
      : frame.totalPct >= 10
      ? { label: "watch this", cls: "text-amber-600" }
      : { label: "healthy", cls: "text-green-700" };

  const left = x + 16 + 240 > containerWidth ? x - 250 : x + 16;

  return (
    <div
      className="absolute z-50 bg-white border border-stone-200 rounded-xl shadow-xl p-3 w-60 pointer-events-none"
      style={{ top: Math.max(y - 60, 4), left }}
    >
      <p className="font-mono text-xs font-semibold text-orange-700 break-all leading-tight mb-1">
        {frame.displayName}
      </p>
      <p className="font-mono text-[10px] text-stone-400 uppercase tracking-widest mb-2">
        {frame.crate} · {frame.layer}
      </p>
      {frame.file && (
        <p className="font-mono text-[9px] text-stone-500 break-all mb-2 border-t border-stone-100 pt-1">
          {frame.file}:{frame.line}
        </p>
      )}
      <div className="grid grid-cols-2 gap-x-3 gap-y-1 text-[11px] border-t border-stone-100 pt-2">
        <StatRow label="self time"  value={`${frame.selfPct.toFixed(2)}%`} />
        <StatRow label="total time" value={`${frame.totalPct.toFixed(2)}%`} />
        <StatRow label="self ns"    value={formatNs(frame.selfNs)} />
        <StatRow label="total ns"   value={formatNs(frame.totalNs)} />
        <StatRow label="samples"    value={frame.samples.toLocaleString()} />
        <StatRow label="depth"      value={String(frame.depth)} />
        {frame.allocBytes != null && (
          <StatRow label="alloc"    value={formatBytes(frame.allocBytes)} />
        )}
        {frame.arcClones != null && frame.arcClones > 0 && (
          <StatRow label="Arc clones" value={frame.arcClones.toLocaleString()} />
        )}
        {frame.pollCount != null && frame.pollCount > 0 && (
          <StatRow label="polls"    value={frame.pollCount.toLocaleString()} />
        )}
        <div className="col-span-2">
          <span className="text-stone-400">status </span>
          <span className={`font-semibold ${hotness.cls}`}>{hotness.label}</span>
        </div>
      </div>
      <p className="text-[10px] text-stone-400 mt-2 border-t border-stone-100 pt-1">
        click to zoom into this call tree
      </p>
    </div>
  );
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span className="text-stone-400 font-mono">{label}</span>
      <span className="text-stone-800 font-mono font-medium text-right">{value}</span>
    </>
  );
}

// ─── Canvas roundRect polyfill ───────────────────────────────────────────────

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number
) {
  if (w < 2 * r) r = w / 2;
  if (h < 2 * r) r = h / 2;
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}
