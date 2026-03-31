"use client";

import React from "react";
import type { ProfileData } from "@/types/profiler";
import { formatNs } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
}

export function AsyncTab({ data }: Props) {
  const hasAsyncData = data.functions.some(f => (f as any).async_task != null);

  if (!hasAsyncData) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-center p-8 bg-[var(--bg1)]">
        <div className="text-[14px] text-[var(--text2)] mb-2 font-mono uppercase tracking-widest">
          no async data
        </div>
        <div className="text-[11px] text-[var(--text3)] max-w-[220px] leading-relaxed">
          null fields in schema — add poll_count, time_pending_ns, wakeup_latency_ns
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
            <th className="text-right px-2 font-normal">Polls</th>
            <th className="text-right px-2 font-normal">Pending</th>
            <th className="text-right px-2 font-normal">Yields</th>
            <th className="text-right px-2 font-normal">Wakeup (p99)</th>
          </tr>
        </thead>
        <tbody>
          {data.functions.map((f, i) => {
            const asyncTask = (f as any).async_task;
            if (!asyncTask) return null;
            return (
              <tr
                key={f.name}
                className={`h-[20px] ${i % 2 === 0 ? "bg-[var(--bg1)]" : "bg-[var(--bg2)]"} hover:bg-[var(--bg3)]`}
              >
                <td className="px-2 truncate max-w-[120px]" title={f.name}>{f.name}</td>
                <td className="px-2 text-right">{asyncTask.poll_count?.toLocaleString()}</td>
                <td className="px-2 text-right">{formatNs(asyncTask.time_pending_ns || 0)}</td>
                <td className="px-2 text-right">{asyncTask.yield_count?.toLocaleString()}</td>
                <td className="px-2 text-right">{formatNs(asyncTask.wakeup_latency_ns?.p99_ns || 0)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
