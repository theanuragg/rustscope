"use client";

import React from "react";
import type { Insight } from "@/types/profiler";
import clsx from "clsx";

interface Props {
  insights: Insight[];
}

const SEVERITY_STYLES: Record<
  Insight["severity"],
  { dot: string; border: string; bg: string; label: string }
> = {
  critical: {
    dot:    "bg-red-500",
    border: "border-red-200",
    bg:     "bg-red-50",
    label:  "critical",
  },
  warn: {
    dot:    "bg-amber-500",
    border: "border-amber-200",
    bg:     "bg-amber-50",
    label:  "warning",
  },
  info: {
    dot:    "bg-blue-400",
    border: "border-blue-200",
    bg:     "bg-blue-50",
    label:  "info",
  },
};

export function InsightPanel({ insights }: Props) {
  if (insights.length === 0) {
    return (
      <div className="bg-white border border-stone-200 rounded-xl px-4 py-3">
        <p className="font-mono text-[11px] text-stone-400 uppercase tracking-widest mb-2">
          findings
        </p>
        <p className="text-sm text-stone-400">No significant issues detected.</p>
      </div>
    );
  }

  return (
    <div className="bg-white border border-stone-200 rounded-xl overflow-hidden">
      <div className="px-4 py-2.5 border-b border-stone-100 bg-stone-50 flex items-center gap-2">
        <span className="font-mono text-[10px] text-stone-400 uppercase tracking-widest">
          findings
        </span>
        <span className="font-mono text-[10px] bg-stone-200 text-stone-600 rounded-full px-2 py-0.5">
          {insights.length}
        </span>
      </div>
      <div className="divide-y divide-stone-100">
        {insights.map((ins, i) => {
          const s = SEVERITY_STYLES[ins.severity];
          return (
            <div key={i} className="px-4 py-3 flex gap-3">
              <div className="flex-shrink-0 mt-1">
                <span className={clsx("w-2 h-2 rounded-full block mt-0.5", s.dot)} />
              </div>
              <div className="min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span
                    className={clsx(
                      "font-mono text-[9px] uppercase tracking-widest px-1.5 py-0.5 rounded border",
                      s.bg, s.border,
                      ins.severity === "critical" ? "text-red-700" :
                      ins.severity === "warn"     ? "text-amber-700" : "text-blue-700"
                    )}
                  >
                    {s.label}
                  </span>
                  <p className="font-sans text-[12px] font-semibold text-stone-800 leading-tight">
                    {ins.title}
                  </p>
                </div>
                <p className="font-sans text-[12px] text-stone-500 leading-relaxed">
                  {ins.body}
                </p>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
