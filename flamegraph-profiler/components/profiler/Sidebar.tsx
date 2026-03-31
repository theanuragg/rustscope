"use client";

import React, { useMemo } from "react";
import type { ProfileData, ProfileFrame } from "@/types/profiler";
import { formatPct, heatColor, formatNs, formatBytes } from "@/lib/parse-profile";
import clsx from "clsx";

interface Props {
  data: ProfileData;
  selectedFunction: string | null;
  onSelect: (name: string) => void;
}

export function Sidebar({ data, selectedFunction, onSelect }: Props) {
  const functions = useMemo(() => {
    const list = (data.raw as any)?.functions || [];
    return [...list].sort((a, b) => {
      const aPct = a.timing?.pct_of_session || a.total_pct || 0;
      const bPct = b.timing?.pct_of_session || b.total_pct || 0;
      return bPct - aPct;
    });
  }, [data]);

  const leakSuspects = useMemo(() => {
    const list = (data.raw as any)?.functions || [];
    return list.filter((f: any) => (f.memory?.net_retained_bytes || 0) > 0);
  }, [data]);

  const selectedData = useMemo(() => {
    if (!selectedFunction) return null;
    return functions.find((f: any) => f.name === selectedFunction);
  }, [selectedFunction, functions]);

  return (
    <div className="w-[260px] flex-shrink-0 bg-[var(--bg-surface)] border-r border-[var(--border)] flex flex-col h-full overflow-hidden font-mono">
      {/* Scrollable List Section */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        {/* Hot Functions */}
        <div className="p-4">
          <h3 className="label-caps mb-4">Hot Functions</h3>
          <div className="space-y-1">
            {functions.slice(0, 50).map((f: any) => {
              const pct = f.timing?.pct_of_session || f.total_pct || 0;
              const isSelected = selectedFunction === f.name;
              return (
                <div
                  key={f.name}
                  onClick={() => onSelect(f.name)}
                  className={clsx(
                    "h-9 px-3 flex items-center gap-3 cursor-pointer transition-all duration-120 group rounded-sm",
                    isSelected ? "bg-[var(--bg-raised)] border-l-2 border-[var(--accent-purple)]" : "hover:bg-[var(--bg-raised)]"
                  )}
                >
                  <div className="flex-1 min-w-0">
                    <div className={clsx(
                      "text-[12px] truncate transition-colors",
                      isSelected ? "text-[var(--text-primary)]" : "text-[var(--text-secondary)] group-hover:text-[var(--text-primary)]"
                    )}>
                      {f.name}
                    </div>
                    <div className="h-[2px] w-full bg-[var(--border)] mt-1 rounded-full overflow-hidden">
                      <div 
                        className="h-full transition-all duration-500" 
                        style={{ width: `${pct}%`, backgroundColor: heatColor(pct) }}
                      />
                    </div>
                  </div>
                  <div className="text-[10px] text-[var(--text-tertiary)] tabular-nums">
                    {formatPct(pct)}
                  </div>
                </div>
              );
            })}
          </div>

          {/* Leak Suspects */}
          {leakSuspects.length > 0 && (
            <div className="mt-8">
              <h3 className="label-caps mb-4 text-[var(--accent-red)]">Leak Suspects</h3>
              <div className="space-y-2">
                {leakSuspects.map((f: any) => (
                  <div 
                    key={f.name}
                    onClick={() => onSelect(f.name)}
                    className={clsx(
                      "p-2 rounded bg-[var(--bg-raised)] cursor-pointer border transition-all",
                      selectedFunction === f.name ? "border-[var(--accent-red)]" : "border-transparent hover:border-[var(--border-bright)]"
                    )}
                  >
                    <div className="text-[11px] text-[var(--text-primary)] truncate mb-1">{f.name}</div>
                    <div className="flex items-center gap-2">
                      <span className="text-[10px] bg-[var(--accent-red)] bg-opacity-10 text-[var(--accent-red)] px-1.5 py-0.5 rounded border border-[var(--accent-red)] border-opacity-20">
                        +{formatBytes(f.memory.net_retained_bytes)} unreleased
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Selected Function Details Drawer */}
      {selectedData && (
        <div className="h-2/3 bg-[var(--bg-raised)] border-t border-[var(--border-bright)] flex flex-col animate-slide-up">
          <div className="p-3 border-b border-[var(--border)] flex items-center justify-between">
            <span className="label-caps">Details</span>
            <button onClick={() => onSelect("")} className="text-[var(--text-tertiary)] hover:text-[var(--text-primary)]">✕</button>
          </div>
          
          <div className="flex-1 overflow-y-auto p-4 space-y-6 custom-scrollbar">
            {/* Header */}
            <div>
              <div className="text-[13px] text-[var(--text-primary)] font-bold break-all mb-2 leading-tight">
                {selectedData.name}
              </div>
              <div 
                className="code-chip inline-block cursor-pointer hover:bg-[var(--bg-surface)]"
                onClick={() => (window as any).onSourceClick?.(selectedData.file, selectedData.line)}
              >
                {selectedData.file}:{selectedData.line}
              </div>
            </div>

            {/* Timing Stats */}
            <section>
              <h4 className="label-caps text-[10px] mb-2 opacity-50">Timing</h4>
              <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                <StatRow label="Total" value={formatNs(selectedData.timing?.total_ns || 0)} />
                <StatRow label="Self" value={formatNs(selectedData.timing?.self_ns || 0)} />
                <StatRow label="Avg" value={formatNs(selectedData.timing?.avg_ns || 0)} />
                <StatRow label="P95" value={formatNs(selectedData.timing?.p95_ns || 0)} />
                <StatRow label="Calls" value={selectedData.call_count?.toLocaleString()} />
                <StatRow label="Recurse" value={selectedData.max_recursion_depth} />
              </div>
            </section>

            {/* Memory Breakdown */}
            <section>
              <h4 className="label-caps text-[10px] mb-2 opacity-50">Memory</h4>
              <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                <StatRow label="Alloc" value={formatBytes(selectedData.memory?.total_alloc_bytes || 0)} />
                <StatRow label="Freed" value={formatBytes(selectedData.memory?.total_dealloc_bytes || 0)} />
                <StatRow label="Net" value={formatBytes(selectedData.memory?.net_retained_bytes || 0)} color={selectedData.memory?.net_retained_bytes > 0 ? "text-[var(--accent-red)]" : ""} />
                <StatRow label="Ops" value={selectedData.memory?.alloc_op_count} />
              </div>
            </section>

            {/* CPU Counters */}
            <section>
              <h4 className="label-caps text-[10px] mb-2 opacity-50">CPU Counters</h4>
              {selectedData.cpu?.instructions > 0 ? (
                <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                  <StatRow label="Instr" value={selectedData.cpu.instructions.toLocaleString()} />
                  <StatRow label="IPC" value={selectedData.cpu.ipc?.toFixed(2)} />
                  <StatRow label="Cycles" value={selectedData.cpu.cpu_cycles.toLocaleString()} />
                  <StatRow label="Misses" value={formatPct(selectedData.cpu.cache_miss_rate * 100)} />
                </div>
              ) : (
                <div className="text-[11px] text-[var(--text-tertiary)] italic">Requires perf counters</div>
              )}
            </section>
          </div>
        </div>
      )}
    </div>
  );
}

function StatRow({ label, value, color }: { label: string; value: any; color?: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[10px] text-[var(--text-tertiary)] uppercase">{label}</span>
      <span className={clsx("text-[11px] font-medium tabular-nums", color || "text-[var(--text-secondary)]")}>
        {value ?? "—"}
      </span>
    </div>
  );
