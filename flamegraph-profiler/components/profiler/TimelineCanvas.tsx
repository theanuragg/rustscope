"use client";

import React, { useRef, useEffect, useState, useCallback, useMemo } from "react";
import type { ProfileData, ProfileFunction } from "@/types/profiler";
import { formatNs } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
  onSelectZone: (zone: any) => void;
  chartType: "flame" | "icicle";
}

const LANE_HEIGHT = 22;
const LABEL_WIDTH = 160;
const RULER_HEIGHT = 18;
const MIN_ZOOM = 0.001;
const MAX_ZOOM = 1000;

// Tracy Colors (hardcoded for Canvas performance and variable resolution)
const COLORS = {
  bg0:     "#0d0d0f",
  bg1:     "#131316",
  bg2:     "#1a1a1f",
  bg3:     "#22222a",
  border:  "#2a2a35",
  border2: "#383845",
  text0:   "#e8e6df",
  text1:   "#9c9a92",
  text2:   "#5c5a55",
  text3:   "#38362f",
  accent:  "#7c6fe0",
  zoneRed:    "#c0392b",
  zoneOrange: "#d35400",
  zoneAmber:  "#b7770d",
  zoneGreen:  "#1e8449",
  zoneTeal:   "#1a7a6e",
  zoneBlue:   "#1f618d",
  zonePurple: "#6c3483",
  zonePink:   "#922b5e",
  zoneGray:   "#4a4a52",
};

const FLAME_PALETTE = [
  COLORS.zoneRed, COLORS.zoneOrange, COLORS.zoneAmber, COLORS.zonePink
];

const ICICLE_PALETTE = [
  COLORS.zoneBlue, COLORS.zoneTeal, COLORS.zoneGreen, COLORS.zonePurple
];

export function TimelineCanvas({ data, onSelectZone, chartType }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  
  const [zoom, setZoom] = useState(1.0);
  const [offsetX, setOffsetX] = useState(0);
  const [hoveredZone, setHoveredZone] = useState<any>(null);
  const [mousePos, setMousePos] = useState({ x: 0, y: 0 });

  const durationNs = data.meta.durationNs;

  // Assign colors to functions based on chartType
  const functionColors = useMemo(() => {
    const map: Record<string, string> = {};
    const palette = chartType === "flame" ? FLAME_PALETTE : ICICLE_PALETTE;
    data.functions?.forEach((f, i) => {
      map[f.name] = palette[i % palette.length];
    });
    return map;
  }, [data.functions, chartType]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.width / dpr;
    const height = canvas.height / dpr;

    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const timelineWidth = width - LABEL_WIDTH;
    const viewNs = durationNs / zoom;
    const scale = timelineWidth / viewNs;

    // 1. Draw X-axis ruler
    ctx.fillStyle = COLORS.bg0;
    ctx.fillRect(LABEL_WIDTH, 0, timelineWidth, RULER_HEIGHT);
    ctx.strokeStyle = COLORS.border2;
    ctx.beginPath();
    ctx.moveTo(LABEL_WIDTH, RULER_HEIGHT);
    ctx.lineTo(width, RULER_HEIGHT);
    ctx.stroke();

    const stepNs = Math.pow(10, Math.floor(Math.log10(viewNs / 5)));
    const startNs = Math.floor(offsetX / scale / stepNs) * stepNs;
    
    ctx.font = "10px JetBrains Mono";
    ctx.fillStyle = COLORS.text2;
    ctx.textAlign = "center";

    for (let ns = startNs; ns < startNs + viewNs + stepNs; ns += stepNs) {
      const x = LABEL_WIDTH + (ns * scale) - (offsetX * scale);
      if (x < LABEL_WIDTH || x > width) continue;
      
      ctx.strokeStyle = COLORS.border2;
      ctx.beginPath();
      ctx.moveTo(x, RULER_HEIGHT - 6);
      ctx.lineTo(x, RULER_HEIGHT);
      ctx.stroke();
      
      ctx.fillText(formatNs(ns), x, RULER_HEIGHT - 8);
    }

    // 2. Draw Lanes
    let currentY = RULER_HEIGHT;

    const drawLane = (label: string, color: string, contentDrawer: (y: number) => void, laneHeight: number = LANE_HEIGHT) => {
      ctx.fillStyle = COLORS.bg1;
      ctx.fillRect(0, currentY, LABEL_WIDTH, laneHeight);
      ctx.strokeStyle = COLORS.border2;
      ctx.strokeRect(0, currentY, LABEL_WIDTH, laneHeight);
      
      ctx.fillStyle = color;
      ctx.textAlign = "right";
      ctx.font = "11px JetBrains Mono";
      ctx.fillText(label, LABEL_WIDTH - 8, currentY + 15);

      ctx.fillStyle = COLORS.bg0;
      ctx.fillRect(LABEL_WIDTH, currentY, timelineWidth, laneHeight);
      ctx.strokeStyle = COLORS.border;
      ctx.strokeRect(LABEL_WIDTH, currentY, timelineWidth, laneHeight);

      contentDrawer(currentY);
      currentY += laneHeight;
    };

    // CPU Usage Lane
    drawLane("CPU USAGE", COLORS.zoneGreen, (y) => {
      ctx.fillStyle = COLORS.zoneGreen;
      ctx.globalAlpha = 0.6;
      ctx.fillRect(LABEL_WIDTH, y + 5, timelineWidth * 0.8, LANE_HEIGHT - 10);
      ctx.globalAlpha = 1.0;
    });

    // Memory Lane
    drawLane("MEMORY", COLORS.zoneBlue, (y) => {
      ctx.fillStyle = COLORS.zoneBlue;
      ctx.globalAlpha = 0.5;
      ctx.fillRect(LABEL_WIDTH, y + 2, timelineWidth * 0.4, LANE_HEIGHT - 4);
      ctx.globalAlpha = 1.0;
      ctx.strokeStyle = COLORS.zoneRed;
      ctx.beginPath();
      ctx.moveTo(LABEL_WIDTH, y + 5);
      ctx.lineTo(width, y + 5);
      ctx.stroke();
    });

    // Real Thread Lanes
    if (data.raw?.call_trees) {
      data.raw.call_trees.forEach((root: any, i: number) => {
        // Calculate max depth for flame graph inversion
        const getMaxDepth = (node: any): number => {
          if (!node.children || node.children.length === 0) return 0;
          return 1 + Math.max(...node.children.map(getMaxDepth));
        };
        const maxDepth = getMaxDepth(root);
        const laneHeight = (maxDepth + 1) * LANE_HEIGHT;

        drawLane(`THREAD ${i}`, COLORS.text1, (y) => {
          const drawNode = (node: any, depth: number, nodeStart: number) => {
            const zx = LABEL_WIDTH + (nodeStart * scale) - (offsetX * scale);
            const zw = (node.duration_ns || node.total_ns || 0) * scale;
            
            // Invert Y based on chartType
            const zy = chartType === "icicle" 
              ? y + (depth * LANE_HEIGHT)
              : y + ((maxDepth - depth) * LANE_HEIGHT);

            if (zx + zw < LABEL_WIDTH || zx > width) return;

            const color = functionColors[node.name] || COLORS.zoneGray;
            ctx.fillStyle = color;
            ctx.fillRect(zx, zy + 1, zw, LANE_HEIGHT - 2);
            
            // Selected/Hovered Highlight
            if (hoveredZone?.name === node.name) {
              ctx.strokeStyle = "#ffffff";
              ctx.lineWidth = 1;
              ctx.strokeRect(zx, zy + 1, zw, LANE_HEIGHT - 2);
            }

            if (zw > 60) {
              ctx.fillStyle = "#ffffff";
              ctx.font = "10px JetBrains Mono";
              ctx.textAlign = "left";
              ctx.fillText(node.name, Math.max(zx + 4, LABEL_WIDTH + 4), zy + 14);
            }

            // BADGES (v4 fields)
            let badgeX = zx + zw - 12;
            const drawBadge = (text: string, bgColor: string) => {
              if (zw < 20) return;
              ctx.fillStyle = bgColor;
              ctx.fillRect(badgeX, zy + 4, 10, 10);
              ctx.fillStyle = "#ffffff";
              ctx.font = "8px JetBrains Mono";
              ctx.textAlign = "center";
              ctx.fillText(text, badgeX + 5, zy + 12);
              badgeX -= 12;
            };

            const func = data.functions?.find(f => f.name === node.name);
            if (func) {
              if (func.sync_contention?.mutex_wait_ns && func.sync_contention.mutex_wait_ns > 0) drawBadge("L", COLORS.zoneRed);
              if (func.generics?.trait_object_calls && func.generics.trait_object_calls > 0) drawBadge("D", COLORS.zonePurple);
              if (func.llvm_hints?.vectorized) drawBadge("V", COLORS.zoneTeal);
              if (func.llvm_hints?.bounds_checks_remaining && func.llvm_hints.bounds_checks_remaining > 0) drawBadge("B", COLORS.zoneAmber);
              if (func.clone_tracking?.clone_count && func.clone_tracking.clone_count > 0) drawBadge("C", COLORS.zoneOrange);
              
              // Drop cost triangle
              if (func.drop_timing?.drop_ns && func.drop_timing.drop_ns > 1000) {
                ctx.fillStyle = COLORS.zoneRed;
                ctx.beginPath();
                ctx.moveTo(zx + zw, zy + 1);
                ctx.lineTo(zx + zw - 6, zy + 1);
                ctx.lineTo(zx + zw, zy + 7);
                ctx.fill();
              }
            }

            // Draw children
            if (node.children) {
              let childStart = nodeStart;
              node.children.forEach((child: any) => {
                drawNode(child, depth + 1, childStart);
                childStart += child.duration_ns || child.total_ns || 0;
              });
            }
          };

          drawNode(root, 0, 0);
        }, laneHeight);
      });
    }

    ctx.restore();
  }, [durationNs, zoom, offsetX, functionColors, data.raw?.call_trees, hoveredZone, chartType, data.functions]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (canvas && container) {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = container.clientWidth * dpr;
      canvas.height = container.clientHeight * dpr;
      canvas.style.width = `${container.clientWidth}px`;
      canvas.style.height = `${container.clientHeight}px`;
      draw();
    }
  }, [draw]);

  useEffect(() => {
    draw();
  }, [draw]);

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      const delta = -e.deltaY;
      const factor = Math.pow(1.1, delta / 100);
      const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor));
      
      const rect = canvasRef.current?.getBoundingClientRect();
      if (rect) {
        const mouseX = e.clientX - rect.left - LABEL_WIDTH;
        if (mouseX > 0) {
          const timelineWidth = rect.width - LABEL_WIDTH;
          const mouseNs = (offsetX + mouseX / (timelineWidth / (durationNs / zoom)));
          const newScale = timelineWidth / (durationNs / newZoom);
          const newOffsetX = mouseNs - (mouseX / newScale);
          setZoom(newZoom);
          setOffsetX(newOffsetX);
        } else {
          setZoom(newZoom);
        }
      }
    } else {
      setOffsetX(prev => prev + e.deltaX * ( (durationNs / zoom) / (canvasRef.current?.clientWidth || 1 - LABEL_WIDTH) ));
    }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setMousePos({ x, y });

    // Simple hit detection for hovered zone (could be improved)
    if (x > LABEL_WIDTH) {
      const timelineWidth = rect.width - LABEL_WIDTH;
      const scale = timelineWidth / (durationNs / zoom);
      const mouseNs = offsetX + (x - LABEL_WIDTH) / scale;
      
      // Find function at this time (mocking for now, would need tree traversal)
      const func = data.functions?.find(f => (f.timing?.total_ns || 0) > mouseNs);
      if (func) setHoveredZone(func);
      else setHoveredZone(null);
    } else {
      setHoveredZone(null);
    }
  };

  return (
    <div ref={containerRef} className="w-full h-full relative overflow-hidden bg-[var(--bg0)]">
      <canvas
        ref={canvasRef}
        onWheel={handleWheel}
        onMouseMove={handleMouseMove}
        onMouseLeave={() => setHoveredZone(null)}
        className="absolute inset-0 cursor-crosshair"
      />

      {/* TOOLTIP */}
      {hoveredZone && (
        <div
          style={{
            left: Math.min(mousePos.x + 10, (containerRef.current?.clientWidth || 0) - 200),
            top: mousePos.y + 10,
          }}
          className="absolute bg-[var(--bg0)] border border-[var(--border2)] p-2 z-[100] pointer-events-none shadow-none font-mono"
        >
          <div className="text-[11px] text-[var(--text0)] mb-0.5">{hoveredZone.name}</div>
          <div className="text-[10px] text-[var(--text2)] mb-1 border-b border-[var(--border2)] pb-1">
            {hoveredZone.file || "unknown source"}:{hoveredZone.line || 0}
          </div>
          <div className="grid grid-cols-2 gap-x-4 text-[10px]">
            <span className="text-[var(--text1)]">total</span>
            <span className="text-[var(--text0)] text-right">{formatNs(hoveredZone.timing?.total_ns || 0)}</span>
            <span className="text-[var(--text1)]">self</span>
            <span className="text-[var(--text0)] text-right">{formatNs(hoveredZone.timing?.self_ns || 0)}</span>
            <span className="text-[var(--text1)]">calls</span>
            <span className="text-[var(--text0)] text-right">{hoveredZone.call_count?.toLocaleString()}</span>
            {hoveredZone.memory?.total_alloc_bytes && (
              <>
                <span className="text-[var(--text1)]">allocs</span>
                <span className="text-[var(--text0)] text-right">{formatNs(hoveredZone.memory.total_alloc_bytes)}</span>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
