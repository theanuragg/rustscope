"use client";

import React from "react";
import type { ProfileData, ProfileMode } from "@/types/profiler";
import { formatNs } from "@/lib/parse-profile";
import clsx from "clsx";

interface Props {
  data: ProfileData;
  activeTab: ProfileMode;
  onTabChange: (tab: ProfileMode) => void;
}

const TABS: Array<{ id: ProfileMode; label: string }> = [
  { id: "timeline",  label: "Timeline" },
  { id: "cpu",       label: "Flamegraph" },
  { id: "memory",    label: "Memory" },
  { id: "threads",   label: "Threads" },
  { id: "allocator", label: "Allocator" },
  { id: "compare",   label: "Compare" },
];

export function Topbar({ data, activeTab, onTabChange }: Props) {
  const durationStr = formatNs(data.meta.durationNs);
  const osArch = `${data.raw?.host?.os || "unknown"} ${data.raw?.host?.arch || ""}`;

  return (
    <div className="h-14 w-full bg-[var(--bg-surface)] border-b border-[var(--border)] flex items-center justify-between px-6 flex-shrink-0">
      <div className="flex items-center gap-10 h-full">
        {/* Logo */}
        <div className="font-mono text-lg font-bold tracking-tighter text-[var(--text-primary)]">
          <span className="text-[var(--accent-purple)] mr-1.5">⬡</span>
          perf.rs
        </div>

        {/* Tabs */}
        <nav className="flex gap-1 h-full items-center">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => onTabChange(tab.id)}
              className={clsx(
                "h-10 px-4 rounded-md font-mono text-[13px] transition-all flex items-center justify-center",
                activeTab === tab.id
                  ? "bg-[var(--bg-raised)] text-[var(--text-primary)] border border-[var(--border-bright)] shadow-sm"
                  : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-raised)]"
              )}
            >
              {tab.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Right Badge */}
      <div className="flex items-center gap-3">
        <div className="code-chip flex items-center gap-2 px-3 py-1.5 bg-[var(--bg-raised)] border-[var(--border-bright)]">
          <span className="text-[var(--text-tertiary)]">session</span>
          <span className="text-[var(--text-primary)] font-medium">{durationStr}</span>
          <span className="w-px h-3 bg-[var(--border)] mx-1" />
          <span className="text-[var(--text-secondary)]">{osArch}</span>
        </div>
        <button 
          onClick={() => window.location.reload()}
          className="text-[11px] font-mono text-[var(--text-tertiary)] hover:text-[var(--accent-purple)] transition-colors px-2"
        >
          LOAD NEW
        </button>
      </div>
    </div>
  );
}
