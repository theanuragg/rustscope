"use client";

import React, {
  useReducer,
  useMemo,
  useRef,
  useEffect,
  useState,
  useCallback,
} from "react";
import type { ProfileData, ProfileFrame } from "@/types/profiler";
import { profilerReducer, initialState } from "@/lib/profiler-reducer";
import { Topbar } from "./Topbar";
import { Sidebar } from "./Sidebar";
import { Toolbar } from "./Toolbar";
import { Breadcrumb } from "./Breadcrumb";
import { FlameCanvas } from "./FlameCanvas";
import clsx from "clsx";

interface Props {
  data: ProfileData;
  onReset: () => void;
}

export function ProfilerView({ data, onReset }: Props) {
  const [state, dispatch] = useReducer(profilerReducer, initialState);
  const containerRef = useRef<HTMLDivElement>(null);
  const [canvasWidth, setCanvasWidth] = useState(800);
  const [selectedFunction, setSelectedFunction] = useState<string | null>(null);

  // Track container width for canvas
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (w && w > 0) setCanvasWidth(Math.floor(w));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const handleTabChange = useCallback((tab: any) => {
    dispatch({ type: "SET_MODE", mode: tab });
  }, []);

  const handleSelectFunction = useCallback((name: string) => {
    setSelectedFunction(name);
  }, []);

  return (
    <div className="flex flex-col h-screen overflow-hidden bg-[var(--bg-primary)] text-[var(--text-primary)] font-mono">
      <Topbar 
        data={data} 
        activeTab={state.mode} 
        onTabChange={handleTabChange} 
      />
      
      <div className="flex flex-1 overflow-hidden">
        <Sidebar 
          data={data} 
          selectedFunction={selectedFunction} 
          onSelect={handleSelectFunction} 
        />
        
        <main className="flex-1 overflow-y-auto bg-[var(--bg-primary)] p-6">
          <TabContent 
            mode={state.mode} 
            data={data} 
            state={state} 
            dispatch={dispatch}
            containerRef={containerRef}
            canvasWidth={canvasWidth}
          />
        </main>
      </div>
    </div>
  );
}

function TabContent({ mode, data, state, dispatch, containerRef, canvasWidth }: any) {
  switch (mode) {
    case "timeline":
      return <TimelineTab data={data} />;
    case "cpu":
      return (
        <FlamegraphTab 
          data={data} 
          state={state} 
          dispatch={dispatch} 
          containerRef={containerRef}
          canvasWidth={canvasWidth}
        />
      );
    case "memory":
      return <div className="text-[var(--text-secondary)]">Memory view coming soon...</div>;
    case "threads":
      return <div className="text-[var(--text-secondary)]">Threads view coming soon...</div>;
    case "allocator":
      return <div className="text-[var(--text-secondary)]">Allocator view coming soon...</div>;
    case "compare":
      return <div className="text-[var(--text-secondary)]">Compare view coming soon...</div>;
    default:
      return null;
  }
}

function TimelineTab({ data }: { data: ProfileData }) {
  return (
    <div className="space-y-6 animate-fade-up">
      <div className="grid grid-cols-4 gap-4">
        <StatCard label="Session Time" value={data.meta.durationNs} format="ns" />
        <StatCard label="Peak Heap" value={data.raw?.session_memory?.peak_heap_bytes || 0} format="bytes" />
        <StatCard label="Total Allocs" value={data.raw?.session_memory?.total_alloc_bytes || 0} format="bytes" />
        <StatCard label="Peak RSS" value={data.raw?.session_memory?.peak_rss_mb || 0} format="mb" />
      </div>
      
      <div className="grid grid-cols-2 gap-6 h-[500px]">
        <div className="bg-[var(--bg-surface)] border border-[var(--border)] rounded-lg p-4">
          <h3 className="label-caps mb-4">Flamegraph Strip</h3>
          <div className="flex flex-col gap-1 overflow-y-auto h-full">
            {/* Horizontal bars representation */}
          </div>
        </div>
        <div className="bg-[var(--bg-surface)] border border-[var(--border)] rounded-lg p-4">
          <h3 className="label-caps mb-4">Call Tree</h3>
          {/* Collapsible tree representation */}
        </div>
      </div>
    </div>
  );
}

function FlamegraphTab({ data, state, dispatch, containerRef, canvasWidth }: any) {
  const frames = useMemo(() => {
    if (state.mode === "cpu")    return data.cpu;
    if (state.mode === "alloc")  return data.alloc ?? [];
    if (state.mode === "offcpu") return data.offcpu ?? [];
    return data.cpu;
  }, [data, state.mode]);

  const handleZoomIn = useCallback(
    (frame: ProfileFrame) => dispatch({ type: "ZOOM_IN", frame }),
    [dispatch]
  );
  const handleHover = useCallback(
    (frame: ProfileFrame | null) => dispatch({ type: "SET_HOVERED", frame }),
    [dispatch]
  );

  return (
    <div className="space-y-4 animate-fade-up">
      <Toolbar state={state} data={data} dispatch={dispatch} />
      <div className="bg-[var(--bg-surface)] border border-[var(--border)] rounded-lg overflow-hidden">
        <div className="px-4 py-2 border-b border-[var(--border)] flex items-center justify-between">
          <Breadcrumb zoomStack={state.zoomStack} isIcicle={state.chartType === "icicle"} dispatch={dispatch} />
          <button 
            onClick={() => dispatch({ type: "RESET_ZOOM" })}
            className="text-[11px] font-mono text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
          >
            RESET ZOOM
          </button>
        </div>
        <div ref={containerRef} className="p-4 overflow-x-auto min-h-[600px]">
          {canvasWidth > 0 && (
            <FlameCanvas
              frames={frames}
              state={state}
              onZoomIn={handleZoomIn}
              onHover={handleHover}
              width={canvasWidth}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function StatCard({ label, value, format }: { label: string; value: number; format: string }) {
  const displayValue = useMemo(() => {
    if (format === "ns") return formatNs(value);
    if (format === "bytes") return formatBytes(value);
    if (format === "mb") return `${value}MB`;
    return value.toLocaleString();
  }, [value, format]);

  return (
    <div className="bg-[var(--bg-surface)] border border-[var(--border)] rounded-lg p-4 hover:border-[var(--border-bright)] transition-colors">
      <div className="label-caps mb-1">{label}</div>
      <div className="text-2xl font-medium text-[var(--text-primary)]">{displayValue}</div>
    </div>
  );
}

import { formatNs, formatBytes } from "@/lib/parse-profile";

// ─── Help tips sidebar ───────────────────────────────────────────────────────

function HelpTips({ mode, isIcicle }: { mode: string; isIcicle: boolean }) {
  const tips = [
    { key: "click", label: "click a frame", desc: "zoom into its call tree" },
    { key: "breadcrumb", label: "breadcrumb trail", desc: "click any ancestor to jump back" },
    { key: "search", label: "search bar", desc: "highlights matching symbols in yellow" },
    { key: "layer", label: "layer pills", desc: "click to show only that layer" },
    { key: "flip", label: "🔥 / 🧊 toggle", desc: "switch between flame and icicle layout" },
  ];

  return (
    <div className="bg-white border border-stone-200 rounded-xl overflow-hidden h-full">
      <div className="px-4 py-2.5 border-b border-stone-100 bg-stone-50">
        <span className="font-mono text-[10px] text-stone-400 uppercase tracking-widest">
          keyboard &amp; mouse
        </span>
      </div>
      <div className="divide-y divide-stone-50">
        {tips.map((t) => (
          <div key={t.key} className="px-4 py-2.5 flex items-start gap-3">
            <code
              className={clsx(
                "font-mono text-[10px] px-1.5 py-0.5 rounded border flex-shrink-0 mt-0.5",
                isIcicle
                  ? "bg-blue-50 border-blue-200 text-blue-700"
                  : "bg-orange-50 border-orange-200 text-orange-700"
              )}
            >
              {t.label}
            </code>
            <p className="font-sans text-[12px] text-stone-500">{t.desc}</p>
          </div>
        ))}
      </div>

      {/* Rust-specific tip */}
      <div
        className={clsx(
          "px-4 py-3 mx-3 mb-3 rounded-lg border text-[11px] font-sans leading-relaxed",
          isIcicle
            ? "bg-blue-50 border-blue-200 text-blue-800"
            : "bg-amber-50 border-amber-200 text-amber-800"
        )}
      >
        <strong>Rust tip:</strong> symbol names are auto-demangled. Hover any frame to see
        crate origin, self vs total ns, and Arc clone / poll counts when available.
      </div>
    </div>
  );
}
