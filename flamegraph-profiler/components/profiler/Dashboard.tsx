"use client";

import React, { useState, useCallback, useEffect } from "react";
import type { ProfileData } from "@/types/profiler";
import { formatNs, formatBytes } from "@/lib/parse-profile";
import { TimelineCanvas } from "./TimelineCanvas";
import { StatsTab } from "./StatsTab";
import { SessionTab } from "./SessionTab";
import { ProjectTab } from "./ProjectTab";
import { MemoryTab } from "./MemoryTab";
import { ZonesTab } from "./ZonesTab";
import { AsyncTab } from "./AsyncTab";
import { LocksTab } from "./LocksTab";
import { CompareTab } from "./CompareTab";
import { FloatingPanel } from "./FloatingPanel";
import { ContextMenu } from "./ContextMenu";

interface Props {
  data: ProfileData;
  onReset: () => void;
}

export function Dashboard({ data, onReset }: Props) {
  const [detailWidth, setDetailWidth] = useState(380);
  const [isResizing, setIsResizing] = useState(false);
  const [activeTab, setActiveTab] = useState("Session");
  const [chartType, setChartType] = useState<"flame" | "icicle">("icicle");
  const [contextMenu, setContextMenu] = useState<{ x: number, y: number } | null>(null);
  const [showFrameInfo, setShowFrameInfo] = useState(true);
  const [showStatsPanel, setShowStatsPanel] = useState(true);

  const startResizing = useCallback(() => {
    setIsResizing(true);
  }, []);

  const stopResizing = useCallback(() => {
    setIsResizing(false);
  }, []);

  const resize = useCallback(
    (e: MouseEvent) => {
      if (isResizing) {
        const newWidth = window.innerWidth - e.clientX;
        if (newWidth > 300 && newWidth < 800) {
          setDetailWidth(newWidth);
        }
      }
    },
    [isResizing]
  );

  useEffect(() => {
    window.addEventListener("mousemove", resize);
    window.addEventListener("mouseup", stopResizing);
    return () => {
      window.removeEventListener("mousemove", resize);
      window.removeEventListener("mouseup", stopResizing);
    };
  }, [resize, stopResizing]);

  // Keyboard Shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (key === "l") {
        // Load JSON
      } else if (key === "f") {
        // Fit
      } else if (key === "1") {
        setActiveTab("Session");
      } else if (key === "2") {
        setActiveTab("Project");
      } else if (key === "3") {
        setActiveTab("Stats");
      } else if (key === "4") {
        setActiveTab("Memory");
      } else if (key === "5") {
        setActiveTab("Zones");
      } else if (key === "6") {
        setActiveTab("Async");
      } else if (key === "7") {
        setActiveTab("Locks");
      } else if (key === "escape") {
        setShowFrameInfo(false);
        setShowStatsPanel(false);
        setContextMenu(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const renderActiveTab = () => {
    switch (activeTab) {
      case "Session":
        return <SessionTab data={data} />;
      case "Project":
        return <ProjectTab data={data} />;
      case "Stats":
        return <StatsTab data={data} onSelectFunction={() => {}} />;
      case "Memory":
        return <MemoryTab data={data} />;
      case "Zones":
        return <ZonesTab data={data} onSelectZone={() => {}} />;
      case "Async":
        return <AsyncTab data={data} />;
      case "Locks":
        return <LocksTab data={data} />;
      case "Compare":
        return <CompareTab current={data} />;
      default:
        return <StatsTab data={data} onSelectFunction={() => {}} />;
    }
  };

  return (
    <div className="flex flex-col w-full h-full bg-[var(--bg0)] select-none" onContextMenu={handleContextMenu}>
      {/* MENUBAR (24px) */}
      <div className="h-[24px] bg-[var(--bg0)] border-bottom border-[var(--border)] flex items-center px-1 shrink-0">
        <div className="flex items-center h-full">
          {["File", "View", "Profiler", "Options", "Help"].map((item) => (
            <div
              key={item}
              className="px-[10px] h-full flex items-center text-[11px] cursor-pointer hover:bg-[var(--bg2)] transition-colors duration-80ms"
            >
              {item}
            </div>
          ))}
        </div>
        <div className="ml-auto flex items-center pr-2 gap-4">
          <span className="text-[var(--text2)] text-[11px]">
            {data.meta.name} / {data.meta.name}
          </span>
          <div className="px-2 py-0.5 bg-[var(--bg2)] border border-[var(--border2)] text-[var(--accent)] text-[10px]">
            {formatNs(data.meta.durationNs)}
          </div>
        </div>
      </div>

      {/* TOOLBAR (28px) */}
      <div className="h-[28px] bg-[var(--bg1)] border-b border-[var(--border2)] flex items-center px-2 shrink-0 gap-2">
        <div className="flex items-center gap-1">
          <button className="tracy-button w-8">▶</button>
          <button className="tracy-button w-8">⏸</button>
          <button className="tracy-button w-8" onClick={onReset}>⏹</button>
        </div>
        
        <div className="w-[1px] h-[16px] bg-[var(--border2)] mx-1" />
        
        <div className="flex items-center gap-1">
          <button className="tracy-button">－</button>
          <div className="text-[11px] px-2 text-[var(--text1)] min-w-[60px] text-center">1.00×</div>
          <button className="tracy-button">＋</button>
          <button className="tracy-button ml-1">Fit</button>
        </div>

        <div className="w-[1px] h-[16px] bg-[var(--border2)] mx-1" />

        <div className="flex items-center gap-1">
          {["Session", "Project", "Stats", "Memory", "Zones", "Async", "Locks", "Compare"].map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`tracy-button px-3 ${tab === activeTab ? "border-b-2 border-b-[var(--accent)]" : ""}`}
            >
              {tab}
            </button>
          ))}
        </div>

        <div className="w-[1px] h-[16px] bg-[var(--border2)] mx-1" />

        <div className="flex items-center gap-1">
          <button
            onClick={() => setChartType("flame")}
            className={`tracy-button px-2 ${chartType === "flame" ? "border-b-2 border-b-[var(--accent)]" : ""}`}
            title="Flame Graph (Bottom-up)"
          >
            🔥
          </button>
          <button
            onClick={() => setChartType("icicle")}
            className={`tracy-button px-2 ${chartType === "icicle" ? "border-b-2 border-b-[var(--accent)]" : ""}`}
            title="Icicle Graph (Top-down)"
          >
            🧊
          </button>
        </div>

        <div className="ml-auto flex items-center gap-2">
          <input
            type="text"
            placeholder="Search..."
            className="w-[140px] h-[20px] bg-[var(--bg0)] border border-[var(--border)] px-2 text-[11px] outline-none focus:border-[var(--accent)]"
          />
          <button className="tracy-button">Load JSON</button>
        </div>
      </div>

      {/* MAIN CONTENT AREA */}
      <div className="flex flex-1 overflow-hidden relative">
        {/* TIMELINE AREA */}
        <div className="flex-1 flex flex-col bg-[var(--bg0)] overflow-hidden">
          <TimelineCanvas data={data} onSelectZone={() => {}} chartType={chartType} />
        </div>

        {/* DRAG HANDLE */}
        <div
          onMouseDown={startResizing}
          className="w-[4px] cursor-ew-resize bg-[var(--bg0)] hover:bg-[var(--accent)] transition-colors duration-80ms z-10"
        />

        {/* DETAIL PANEL */}
        <div
          style={{ width: detailWidth }}
          className="bg-[var(--bg1)] border-l border-[var(--border2)] flex flex-col"
        >
          {/* Detail panel tabs */}
          <div className="h-[22px] bg-[var(--bg1)] border-b border-[var(--border2)] flex items-center">
            {["Session", "Project", "Stats", "Memory", "Zones", "Async", "Locks", "Compare"].map((tab) => (
              <div
                key={tab}
                onClick={() => setActiveTab(tab)}
                className={`px-[10px] h-full flex items-center text-[11px] cursor-pointer transition-colors duration-80ms ${
                  activeTab === tab
                    ? "text-[var(--text0)] border-b-2 border-[var(--accent)]"
                    : "text-[var(--text1)] hover:text-[var(--text0)]"
                }`}
              >
                {tab}
              </div>
            ))}
          </div>
          <div className="flex-1 overflow-hidden">
            {renderActiveTab()}
          </div>
        </div>

        {/* FLOATING PANELS */}
        {showFrameInfo && (
          <FloatingPanel
            title="frame info"
            onClose={() => setShowFrameInfo(false)}
            initialX={window.innerWidth - detailWidth - 300}
            initialY={60}
          >
            <div className="flex flex-col gap-1 text-[11px]">
              <div className="flex justify-between">
                <span className="text-[var(--text1)]">duration</span>
                <span className="text-[var(--text0)]">{formatNs(data.meta.durationNs)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-[var(--text1)]">peak heap</span>
                <span className="text-[var(--text0)]">{formatBytes(data.raw?.session_memory?.peak_heap_bytes || 0)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-[var(--text1)]">host</span>
                <span className="text-[var(--text0)]">{data.raw?.meta?.os || "macos"}</span>
              </div>
            </div>
          </FloatingPanel>
        )}

        {showStatsPanel && (
          <FloatingPanel
            title="statistics"
            onClose={() => setShowStatsPanel(false)}
            initialX={window.innerWidth - detailWidth - 300}
            initialY={140}
          >
            <div className="flex flex-col gap-1 text-[11px]">
              {data.functions.slice(0, 5).map(f => (
                <div key={f.name} className="flex justify-between h-[18px] hover:bg-[var(--bg2)] cursor-pointer px-1">
                  <span className="truncate max-w-[120px] text-[var(--text1)]">{f.name}</span>
                  <span className="text-[var(--text0)]">{formatNs(f.timing?.total_ns || 0)}</span>
                </div>
              ))}
            </div>
          </FloatingPanel>
        )}
      </div>

      {/* STATUSBAR (20px) */}
      <div className="h-[20px] bg-[var(--bg0)] border-t border-[var(--border)] flex items-center px-2 shrink-0 justify-between text-[11px] text-[var(--text1)]">
        <div className="flex items-center gap-3">
          <span>Functions: {data.functions.length}</span>
          <span className="text-[var(--text3)]">·</span>
          <span>Frames: {data.raw?.call_trees?.length || 0}</span>
          <span className="text-[var(--text3)]">·</span>
          <span>Session: {formatNs(data.meta.durationNs)}</span>
        </div>
        <div className="flex items-center gap-3">
          <span>Peak heap: {formatBytes(data.raw?.session_memory?.peak_heap_bytes || 0)}</span>
          <span className="text-[var(--text3)]">·</span>
          <span>Allocs: {data.raw?.session_memory?.total_alloc_count?.toLocaleString() || 0}</span>
          <span className="text-[var(--text3)]">·</span>
          <span>OS: {data.raw?.meta?.os || "macos"}</span>
        </div>
      </div>

      {/* CONTEXT MENU */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
