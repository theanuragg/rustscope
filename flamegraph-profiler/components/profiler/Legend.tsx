"use client";

import React from "react";
import type { ChartType } from "@/types/profiler";
import { legendEntries } from "@/lib/colors";
import clsx from "clsx";

interface Props {
  chartType: ChartType;
}

export function Legend({ chartType }: Props) {
  const entries = legendEntries(chartType);
  const isIcicle = chartType === "icicle";

  return (
    <div
      className={clsx(
        "flex items-center gap-3 px-3 py-2 border-t flex-wrap",
        isIcicle ? "bg-blue-50/40 border-blue-100" : "bg-stone-50 border-stone-100"
      )}
    >
      <span className="font-mono text-[10px] text-stone-400 uppercase tracking-widest">
        layer
      </span>
      {entries.map((e) => (
        <div key={e.layer} className="flex items-center gap-1.5">
          <span
            className="w-2.5 h-2.5 rounded-sm flex-shrink-0"
            style={{ background: e.bg }}
          />
          <span className="font-mono text-[10px] text-stone-500">{e.label}</span>
        </div>
      ))}
    </div>
  );
}
