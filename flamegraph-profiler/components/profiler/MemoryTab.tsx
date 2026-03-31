"use client";

import React from "react";
import type { ProfileData } from "@/types/profiler";
import { formatBytes, formatPct } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
}

export function MemoryTab({ data }: Props) {
  const totalAllocated = data.raw?.session_memory?.total_alloc_bytes || 0;
  const totalDeallocated = data.raw?.session_memory?.total_dealloc_bytes || 0;
  const retained = totalAllocated - totalDeallocated;

  const leakSuspects = data.functions.filter(f => (f.memory?.net_retained_bytes || 0) > 0);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Top Metrics Row */}
      <div className="p-2 border-b border-[var(--border2)] flex gap-4 text-[11px] shrink-0">
        <div className="flex gap-2">
          <span className="text-[var(--text1)]">allocated</span>
          <span className="text-[var(--text0)]">{formatBytes(totalAllocated)}</span>
        </div>
        <div className="flex gap-2">
          <span className="text-[var(--text1)]">freed</span>
          <span className="text-[var(--text0)]">{formatBytes(totalDeallocated)}</span>
        </div>
        <div className="flex gap-2">
          <span className="text-[var(--text1)]">retained</span>
          <span className={retained > 0 ? "text-[var(--leak)]" : "text-[var(--ok)]"}>
            {formatBytes(retained)}
          </span>
        </div>
      </div>

      {/* Allocation Table */}
      <div className="flex-1 overflow-auto bg-[var(--bg0)]">
        <table className="w-full border-collapse text-[11px]">
          <thead className="sticky top-0 bg-[var(--bg0)] text-[var(--text1)] tracy-label-caps h-[18px]">
            <tr>
              <th className="text-left px-2 font-normal">Function</th>
              <th className="text-right px-2 font-normal">Allocs</th>
              <th className="text-right px-2 font-normal">Deallocs</th>
              <th className="text-right px-2 font-normal">Δ</th>
              <th className="text-right px-2 font-normal">Peak</th>
              <th className="text-right px-2 font-normal">Mean/call</th>
            </tr>
          </thead>
          <tbody>
            {data.functions.map((f, i) => {
              const delta = f.memory?.net_retained_bytes || 0;
              return (
                <tr
                  key={f.name}
                  className={`h-[20px] ${i % 2 === 0 ? "bg-[var(--bg1)]" : "bg-[var(--bg2)]"} hover:bg-[var(--bg3)]`}
                >
                  <td className="px-2 truncate max-w-[120px]" title={f.name}>{f.name}</td>
                  <td className="px-2 text-right">{f.memory?.alloc_count?.toLocaleString()}</td>
                  <td className="px-2 text-right">{f.memory?.dealloc_count?.toLocaleString()}</td>
                  <td className={`px-2 text-right ${delta > 0 ? "text-[var(--leak)]" : "text-[var(--ok)]"}`}>
                    {formatBytes(delta)}
                  </td>
                  <td className="px-2 text-right">{formatBytes(f.memory?.peak_delta_bytes || 0)}</td>
                  <td className="px-2 text-right">{formatBytes(f.memory?.mean_alloc_per_call || 0)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Leak Suspects */}
      {leakSuspects.length > 0 && (
        <div className="h-[100px] border-t border-[var(--border2)] bg-[var(--bg1)] p-2 shrink-0">
          <div className="text-[var(--leak)] tracy-label-caps mb-1 uppercase">⚠ leak suspects</div>
          <div className="overflow-auto h-full pb-4">
            {leakSuspects.map(f => (
              <div key={f.name} className="flex justify-between items-center text-[11px] h-[18px] bg-[var(--leak)] bg-opacity-[0.08] px-2 mb-1">
                <span className="truncate">{f.name}</span>
                <span className="text-[var(--leak)] font-medium">{formatBytes(f.memory?.net_retained_bytes || 0)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
