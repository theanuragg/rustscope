"use client";

import React from "react";
import type { ProfileData, ProfileMode } from "@/types/profiler";
import { formatNs, formatBytes } from "@/lib/demangle";
import clsx from "clsx";

interface Props {
  data: ProfileData;
  mode: ProfileMode;
}

export function MetricsBar({ data, mode }: Props) {
  const { meta } = data;
  const frames = mode === "cpu" ? data.cpu : mode === "alloc" ? (data.alloc ?? []) : (data.offcpu ?? []);
  const bySelf = [...frames].sort((a, b) => b.selfPct - a.selfPct);
  const topSelf = bySelf[0];

  const metrics: Array<{
    label: string;
    value: string;
    sub?: string;
    accent?: "red" | "orange" | "amber" | "default";
  }> = [
    {
      label: "duration",
      value: formatNs(meta.durationNs),
      sub: meta.tool ?? "profiler",
    },
    {
      label: "total samples",
      value: meta.totalSamples.toLocaleString(),
      sub: meta.sampleHz ? `@ ${meta.sampleHz} Hz` : undefined,
    },
    {
      label: "hottest frame",
      value: topSelf ? `${topSelf.selfPct.toFixed(1)}%` : "—",
      sub: topSelf?.displayName.split("::").pop() ?? "—",
      accent: topSelf?.selfPct >= 20 ? "red" : topSelf?.selfPct >= 10 ? "orange" : "amber",
    },
    ...(meta.peakHeapBytes != null && mode === "alloc"
      ? [{ label: "peak heap", value: formatBytes(meta.peakHeapBytes), sub: "max live", accent: "orange" as const }]
      : []),
    ...(meta.rustVersion
      ? [{ label: "rust", value: meta.rustVersion, sub: "toolchain" }]
      : []),
  ];

  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-2.5 mb-4">
      {metrics.map((m) => (
        <div
          key={m.label}
          className="bg-white border border-stone-200 rounded-xl px-4 py-3"
        >
          <p className="font-mono text-[10px] text-stone-400 uppercase tracking-widest mb-1">
            {m.label}
          </p>
          <p
            className={clsx(
              "font-mono text-xl font-semibold leading-tight tracking-tight",
              m.accent === "red"    && "text-red-600",
              m.accent === "orange" && "text-orange-600",
              m.accent === "amber"  && "text-amber-600",
              !m.accent             && "text-stone-800"
            )}
          >
            {m.value}
          </p>
          {m.sub && (
            <p className="font-mono text-[10px] text-stone-400 mt-0.5 truncate">{m.sub}</p>
          )}
        </div>
      ))}
    </div>
  );
}
