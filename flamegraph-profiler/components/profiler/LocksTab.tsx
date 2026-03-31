"use client";

import React from "react";
import type { ProfileData } from "@/types/profiler";
import { formatNs } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
}

export function LocksTab({ data }: Props) {
  const hasLockData = data.functions.some(f => (f as any).sync_contention != null);

  if (!hasLockData) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-center p-8 bg-[var(--bg1)]">
        <div className="text-[14px] text-[var(--text2)] mb-2 font-mono uppercase tracking-widest">
          no contention data recorded
        </div>
        <div className="text-[11px] text-[var(--text3)] max-w-[220px] leading-relaxed">
          this requires sync_contention fields in schema v4
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden bg-[var(--bg0)]">
      <table className="w-full border-collapse text-[11px]">
        <thead className="sticky top-0 bg-[var(--bg0)] text-[var(--text1)] tracy-label-caps h-[18px]">
          <tr>
            <th className="text-left px-2 font-normal">Function</th>
            <th className="text-right px-2 font-normal">Wait (ns)</th>
            <th className="text-right px-2 font-normal">Contention</th>
            <th className="text-right px-2 font-normal">Type</th>
          </tr>
        </thead>
        <tbody>
          {data.functions.map((f, i) => {
            const contention = (f as any).sync_contention;
            if (!contention) return null;
            return (
              <tr
                key={f.name}
                className={`h-[20px] ${i % 2 === 0 ? "bg-[var(--bg1)]" : "bg-[var(--bg2)]"} hover:bg-[var(--bg3)]`}
              >
                <td className="px-2 truncate max-w-[120px]" title={f.name}>{f.name}</td>
                <td className="px-2 text-right">{formatNs(contention.mutex_wait_ns || 0)}</td>
                <td className="px-2 text-right">{contention.mutex_contention_count?.toLocaleString()}</td>
                <td className="px-2 text-right text-[var(--text2)]">Mutex</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
