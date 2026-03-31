"use client";

import React, { useState } from "react";
import type { ProfileData, ProfileFunction } from "@/types/profiler";
import { formatNs, formatBytes, formatPct, heatColor } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
  onSelectFunction: (name: string) => void;
}

export function StatsTab({ data, onSelectFunction }: Props) {
  const [selectedFunc, setSelectedFunc] = useState<string | null>(null);

  const sortedFunctions = [...data.functions].sort((a, b) => (b.timing?.total_ns || 0) - (a.timing?.total_ns || 0));

  const selectedFunctionData = data.functions.find(f => f.name === selectedFunc);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Hot Functions Table */}
      <div className="flex-1 overflow-auto bg-[var(--bg0)]">
        <table className="w-full border-collapse text-[11px]">
          <thead className="sticky top-0 bg-[var(--bg0)] text-[var(--text1)] tracy-label-caps h-[18px]">
            <tr>
              <th className="text-left px-2 font-normal">Name</th>
              <th className="text-right px-2 font-normal">Total</th>
              <th className="text-right px-2 font-normal">Self</th>
              <th className="text-right px-2 font-normal">%</th>
              <th className="text-right px-2 font-normal">Calls</th>
              <th className="text-right px-2 font-normal">Allocs</th>
            </tr>
          </thead>
          <tbody>
            {sortedFunctions.map((f, i) => {
              const totalPct = f.timing?.pct_of_session || 0;
              const isSelected = selectedFunc === f.name;
              return (
                <tr
                  key={f.name}
                  onClick={() => {
                    setSelectedFunc(f.name);
                    onSelectFunction(f.name);
                  }}
                  className={`h-[20px] cursor-pointer group relative ${
                    i % 2 === 0 ? "bg-[var(--bg1)]" : "bg-[var(--bg2)]"
                  } ${isSelected ? "tracy-row-selected" : "hover:bg-[var(--bg3)]"}`}
                >
                  <td className="px-2 truncate max-w-[120px]" title={f.name}>
                    {f.name}
                  </td>
                  <td className="px-2 text-right">{formatNs(f.timing?.total_ns || 0)}</td>
                  <td className="px-2 text-right">{formatNs(f.timing?.self_ns || 0)}</td>
                  <td className="px-2 text-right relative">
                    <div
                      className="absolute inset-y-0 right-0 pointer-events-none"
                      style={{
                        width: `${totalPct}%`,
                        backgroundColor: heatColor(totalPct),
                        opacity: 0.25,
                      }}
                    />
                    <span className="relative z-10">{formatPct(totalPct)}</span>
                  </td>
                  <td className="px-2 text-right">{f.call_count?.toLocaleString()}</td>
                  <td className="px-2 text-right">{formatBytes(f.memory?.total_alloc_bytes || 0)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Timing Breakdown */}
      {selectedFunctionData && (
        <div className="h-[120px] border-t border-[var(--border2)] bg-[var(--bg1)] p-2 shrink-0">
          <div className="tracy-label-caps mb-1">timing breakdown</div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-[11px]">
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">avg</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.avg_ns || 0)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">min</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.min_ns || 0)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">max</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.max_ns || 0)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">p50</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.p50_ns || 0)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">p95</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.p95_ns || 0)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-[var(--text1)]">p99</span>
              <span className="text-[var(--text0)]">{formatNs(selectedFunctionData.timing?.p99_ns || 0)}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
