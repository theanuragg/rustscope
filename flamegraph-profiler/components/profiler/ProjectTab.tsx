"use client";

import React from "react";
import type { ProfileData, ProfileRollup } from "@/types/profiler";
import { formatPct } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
}

export function ProjectTab({ data }: Props) {
  return (
    <div className="flex flex-col h-full overflow-auto bg-[var(--bg0)] p-3 gap-3">
      <RollupSection title="crate hotspots" rows={data.crateRollups || []} />
      <RollupSection title="module hotspots" rows={data.moduleRollups || []} />
    </div>
  );
}

function RollupSection({ title, rows }: { title: string; rows: ProfileRollup[] }) {
  return (
    <section className="bg-[var(--bg1)] border border-[var(--border2)] p-3">
      <div className="tracy-label-caps mb-2">{title}</div>
      <div className="space-y-1">
        {rows.length === 0 ? (
          <div className="text-[11px] text-[var(--text2)]">No aggregated hotspots available</div>
        ) : (
          rows.map((row) => (
            <div key={row.name} className="grid grid-cols-[1fr_auto_auto_auto] gap-3 text-[11px] border-b border-[var(--border)] py-1">
              <span className="truncate text-[var(--text0)]">{row.name}</span>
              <span className="text-[var(--text1)]">{formatPct(row.total_pct || 0)}</span>
              <span className="text-[var(--text1)]">{row.calls.toLocaleString()} calls</span>
              <span className="text-[var(--text2)]">{row.function_count} fns</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
