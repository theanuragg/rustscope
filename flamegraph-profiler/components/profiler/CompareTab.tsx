"use client";

import React, { useMemo, useRef, useState } from "react";
import type { ProfileData, ProfileFunction } from "@/types/profiler";
import { formatBytes, formatNs } from "@/lib/parse-profile";

interface Props {
  current: ProfileData;
}

interface DiffRow {
  name: string;
  current?: ProfileFunction;
  baseline?: ProfileFunction;
  currentTotalNs: number;
  baselineTotalNs: number;
  deltaNs: number;
  deltaPct: number | null;
  currentCalls: number;
  baselineCalls: number;
  currentRetainedBytes: number;
  baselineRetainedBytes: number;
  retainedDeltaBytes: number;
  status: "regression" | "improvement" | "new" | "removed" | "stable";
}

interface SummaryMetrics {
  regressions: number;
  improvements: number;
  newHotspots: number;
  removedHotspots: number;
}

export function CompareTab({ current }: Props) {
  const [baseline, setBaseline] = useState<ProfileData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const rows = useMemo(() => {
    if (!baseline) return [];

    const currentMap = new Map((current.functions || []).map((fn) => [fn.name, fn]));
    const baselineMap = new Map((baseline.functions || []).map((fn) => [fn.name, fn]));
    const names:any = new Set(Array.from(currentMap.keys()).concat(Array.from(baselineMap.keys())));

    return [...names]
      .map((name): DiffRow => {
        const currentFn = currentMap.get(name);
        const baselineFn = baselineMap.get(name);
        const currentTotalNs = currentFn?.timing?.total_ns || 0;
        const baselineTotalNs = baselineFn?.timing?.total_ns || 0;
        const deltaNs = currentTotalNs - baselineTotalNs;
        const deltaPct =
          baselineTotalNs > 0 ? (deltaNs / baselineTotalNs) * 100 : currentTotalNs > 0 ? 100 : null;
        const currentRetainedBytes = currentFn?.memory?.net_retained_bytes || 0;
        const baselineRetainedBytes = baselineFn?.memory?.net_retained_bytes || 0;
        const retainedDeltaBytes = currentRetainedBytes - baselineRetainedBytes;

        let status: DiffRow["status"] = "stable";
        if (!baselineFn && currentFn) {
          status = "new";
        } else if (baselineFn && !currentFn) {
          status = "removed";
        } else if (deltaPct !== null && deltaPct >= 15 && deltaNs > 500_000) {
          status = "regression";
        } else if (deltaPct !== null && deltaPct <= -15 && deltaNs < -500_000) {
          status = "improvement";
        }

        return {
          name,
          current: currentFn,
          baseline: baselineFn,
          currentTotalNs,
          baselineTotalNs,
          deltaNs,
          deltaPct,
          currentCalls: currentFn?.call_count || 0,
          baselineCalls: baselineFn?.call_count || 0,
          currentRetainedBytes,
          baselineRetainedBytes,
          retainedDeltaBytes,
          status,
        };
      })
      .sort((a, b) => {
        const aScore = Math.abs(a.deltaNs) + Math.max(0, a.currentTotalNs - a.baselineTotalNs);
        const bScore = Math.abs(b.deltaNs) + Math.max(0, b.currentTotalNs - b.baselineTotalNs);
        return bScore - aScore;
      });
  }, [baseline, current.functions]);

  const summary = useMemo<SummaryMetrics>(() => {
    return rows.reduce(
      (acc, row) => {
        if (row.status === "regression") acc.regressions += 1;
        if (row.status === "improvement") acc.improvements += 1;
        if (row.status === "new" && row.currentTotalNs > 1_000_000) acc.newHotspots += 1;
        if (row.status === "removed" && row.baselineTotalNs > 1_000_000) acc.removedHotspots += 1;
        return acc;
      },
      { regressions: 0, improvements: 0, newHotspots: 0, removedHotspots: 0 }
    );
  }, [rows]);

  const topRegressions = useMemo(
    () => rows.filter((row) => row.status === "regression" || row.status === "new").slice(0, 8),
    [rows]
  );

  const handleFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    setLoading(true);
    setError(null);

    try {
      const text = await file.text();
      const json = JSON.parse(text);
      const { parseSession } = await import("@/lib/parse-profile");
      const result = parseSession(json);

      if (!result.ok || !result.data) {
        setBaseline(null);
        setError(result.error || "Failed to parse baseline profile");
        return;
      }

      setBaseline(result.data);
    } catch {
      setBaseline(null);
      setError("Failed to read baseline JSON");
    } finally {
      setLoading(false);
      event.target.value = "";
    }
  };

  const sessionDurationDelta = baseline
    ? current.meta.durationNs - baseline.meta.durationNs
    : 0;

  const peakHeapDelta = baseline
    ? (current.raw?.session_memory?.peak_heap_bytes || 0) -
      (baseline.raw?.session_memory?.peak_heap_bytes || 0)
    : 0;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <input
        ref={inputRef}
        type="file"
        accept=".json"
        className="hidden"
        onChange={handleFileChange}
      />

      <div className="p-3 border-b border-[var(--border2)] bg-[var(--bg1)] flex items-center justify-between gap-3 shrink-0">
        <div>
          <div className="tracy-label-caps mb-1">Regression Compare</div>
          <div className="text-[11px] text-[var(--text1)]">
            Current: {current.meta.name}
            {baseline ? ` vs ${baseline.meta.name}` : " | Load a baseline JSON to compare"}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {baseline && (
            <button className="tracy-button" onClick={() => setBaseline(null)}>
              Clear
            </button>
          )}
          <button className="tracy-button" onClick={() => inputRef.current?.click()}>
            {loading ? "Loading..." : baseline ? "Replace Baseline" : "Load Baseline"}
          </button>
        </div>
      </div>

      {error && (
        <div className="px-3 py-2 text-[11px] text-[var(--leak)] border-b border-[var(--border2)] bg-[var(--bg1)]">
          {error}
        </div>
      )}

      {!baseline ? (
        <div className="flex-1 flex items-center justify-center bg-[var(--bg0)] text-[var(--text1)] text-[11px]">
          Compare mode highlights regressions, improvements, and new hotspots between two RustScope sessions.
        </div>
      ) : (
        <>
          <div className="grid grid-cols-4 gap-px bg-[var(--border2)] shrink-0">
            <SummaryCard label="Session Delta" value={formatSignedNs(sessionDurationDelta)} tone={sessionDurationDelta > 0 ? "bad" : "good"} />
            <SummaryCard label="Peak Heap Delta" value={formatSignedBytes(peakHeapDelta)} tone={peakHeapDelta > 0 ? "bad" : "good"} />
            <SummaryCard label="Regressions" value={summary.regressions.toString()} tone={summary.regressions > 0 ? "bad" : "neutral"} />
            <SummaryCard label="New Hotspots" value={summary.newHotspots.toString()} tone={summary.newHotspots > 0 ? "bad" : "neutral"} />
          </div>

          {topRegressions.length > 0 && (
            <div className="p-3 border-b border-[var(--border2)] bg-[var(--bg1)] shrink-0">
              <div className="tracy-label-caps mb-2">Top Changes</div>
              <div className="flex flex-wrap gap-2">
                {topRegressions.map((row) => (
                  <div
                    key={row.name}
                    className={`px-2 py-1 text-[11px] border ${
                      row.status === "regression" || row.status === "new"
                        ? "border-[var(--leak)] text-[var(--text0)] bg-[rgba(231,76,60,0.08)]"
                        : "border-[var(--border2)] text-[var(--text1)] bg-[var(--bg0)]"
                    }`}
                  >
                    {row.name} {formatSignedNs(row.deltaNs)}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="flex-1 overflow-auto bg-[var(--bg0)]">
            <table className="w-full border-collapse text-[11px]">
              <thead className="sticky top-0 bg-[var(--bg0)] text-[var(--text1)] tracy-label-caps h-[18px]">
                <tr>
                  <th className="text-left px-2 font-normal">Function</th>
                  <th className="text-left px-2 font-normal">Status</th>
                  <th className="text-right px-2 font-normal">Current</th>
                  <th className="text-right px-2 font-normal">Baseline</th>
                  <th className="text-right px-2 font-normal">Delta</th>
                  <th className="text-right px-2 font-normal">Calls</th>
                  <th className="text-right px-2 font-normal">Retained Δ</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row, index) => (
                  <tr
                    key={row.name}
                    className={`${index % 2 === 0 ? "bg-[var(--bg1)]" : "bg-[var(--bg2)]"} hover:bg-[var(--bg3)]`}
                  >
                    <td className="px-2 truncate max-w-[220px]" title={row.name}>
                      {row.name}
                    </td>
                    <td className={`px-2 ${statusColor(row.status)}`}>{statusLabel(row.status)}</td>
                    <td className="px-2 text-right">{formatNs(row.currentTotalNs)}</td>
                    <td className="px-2 text-right">{formatNs(row.baselineTotalNs)}</td>
                    <td className={`px-2 text-right ${deltaColor(row.deltaNs)}`}>
                      {formatSignedNs(row.deltaNs)}
                      {row.deltaPct !== null ? ` (${formatSignedPct(row.deltaPct)})` : ""}
                    </td>
                    <td className="px-2 text-right">
                      {row.currentCalls.toLocaleString()} / {row.baselineCalls.toLocaleString()}
                    </td>
                    <td className={`px-2 text-right ${deltaColor(row.retainedDeltaBytes)}`}>
                      {formatSignedBytes(row.retainedDeltaBytes)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}

function SummaryCard({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "good" | "bad" | "neutral";
}) {
  const toneClass =
    tone === "bad"
      ? "text-[var(--leak)]"
      : tone === "good"
        ? "text-[var(--ok)]"
        : "text-[var(--text0)]";

  return (
    <div className="bg-[var(--bg1)] px-3 py-3">
      <div className="tracy-label-caps mb-1">{label}</div>
      <div className={`text-[13px] ${toneClass}`}>{value}</div>
    </div>
  );
}

function deltaColor(value: number) {
  if (value > 0) return "text-[var(--leak)]";
  if (value < 0) return "text-[var(--ok)]";
  return "text-[var(--text0)]";
}

function statusColor(status: DiffRow["status"]) {
  switch (status) {
    case "regression":
    case "new":
      return "text-[var(--leak)]";
    case "improvement":
    case "removed":
      return "text-[var(--ok)]";
    default:
      return "text-[var(--text1)]";
  }
}

function statusLabel(status: DiffRow["status"]) {
  switch (status) {
    case "regression":
      return "Slower";
    case "improvement":
      return "Faster";
    case "new":
      return "New";
    case "removed":
      return "Removed";
    default:
      return "Stable";
  }
}

function formatSignedNs(value: number) {
  if (value === 0) return formatNs(0);
  const sign = value > 0 ? "+" : "";
  return `${sign}${formatNs(value)}`;
}

function formatSignedBytes(value: number) {
  if (value === 0) return formatBytes(0);
  const sign = value > 0 ? "+" : "";
  return `${sign}${formatBytes(value)}`;
}

function formatSignedPct(value: number) {
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)}%`;
}
