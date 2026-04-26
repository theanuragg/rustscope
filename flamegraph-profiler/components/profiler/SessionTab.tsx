"use client";

import React, { useMemo, useState } from "react";
import type { ProfileData, ProfileHotspotSnapshot, ProfileRollup } from "@/types/profiler";
import { formatBytes, formatNs, formatPct } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
}

export function SessionTab({ data }: Props) {
  const processSummary = data.raw?.process_summary;
  const sessionMeta = data.raw?.session_meta;
  const samples = data.samples || [];
  const events = data.events || [];
  const hotspotSnapshots = data.hotspotSnapshots || [];
  const [selectedEventIndex, setSelectedEventIndex] = useState(0);

  const peakSyscalls = samples.reduce((max, sample) => Math.max(max, sample.syscalls_per_sec), 0);
  const peakThreads = samples.reduce((max, sample) => Math.max(max, sample.threads), 0);
  const peakFds = samples.reduce((max, sample) => Math.max(max, sample.open_fds), 0);
  const selectedEvent = events[selectedEventIndex] || null;

  const correlatedSnapshot = useMemo(() => {
    if (!selectedEvent || hotspotSnapshots.length === 0) return null;
    return hotspotSnapshots.reduce<ProfileHotspotSnapshot | null>((closest, snapshot) => {
      if (!closest) return snapshot;
      const currentDelta = Math.abs(snapshot.ts - selectedEvent.ts);
      const closestDelta = Math.abs(closest.ts - selectedEvent.ts);
      return currentDelta < closestDelta ? snapshot : closest;
    }, null);
  }, [selectedEvent, hotspotSnapshots]);

  return (
    <div className="flex flex-col h-full overflow-auto bg-[var(--bg0)] p-3 gap-3">
      <section className="grid grid-cols-2 gap-3">
        <MetricCard label="duration" value={formatNs(data.meta.durationNs)} />
        <MetricCard label="samples" value={samples.length.toLocaleString()} />
        <MetricCard label="cpu avg / peak" value={`${processSummary?.cpu_avg_pct?.toFixed(1) || "0.0"}% / ${processSummary?.cpu_peak_pct?.toFixed(1) || "0.0"}%`} />
        <MetricCard label="heap avg / peak" value={`${processSummary?.heap_avg_mb?.toFixed(1) || "0.0"} MB / ${processSummary?.heap_peak_mb?.toFixed(1) || "0.0"} MB`} />
        <MetricCard label="peak open fds" value={peakFds.toLocaleString()} />
        <MetricCard label="peak threads" value={peakThreads.toLocaleString()} />
        <MetricCard label="peak syscalls/s" value={peakSyscalls.toLocaleString()} />
        <MetricCard label="peak rss" value={formatBytes(data.raw?.session_memory?.peak_heap_bytes || 0)} />
      </section>

      <section className="bg-[var(--bg1)] border border-[var(--border2)] p-3">
        <div className="tracy-label-caps mb-2">session metadata</div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-[11px]">
          <MetaRow label="project" value={sessionMeta?.project || data.meta.name} />
          <MetaRow label="target" value={sessionMeta?.target_binary || "unknown"} />
          <MetaRow label="host" value={data.raw?.host?.os || "unknown"} />
          <MetaRow label="arch" value={data.raw?.host?.arch || "unknown"} />
          <MetaRow label="start" value={sessionMeta?.start_ts_ms?.toString() || "—"} />
          <MetaRow label="end" value={sessionMeta?.end_ts_ms?.toString() || "—"} />
        </div>
      </section>

      <section className="grid grid-cols-[1.2fr_1fr] gap-3">
        <div className="bg-[var(--bg1)] border border-[var(--border2)] p-3">
        <div className="tracy-label-caps mb-2">event log</div>
        <div className="space-y-1 text-[11px]">
          {events.length === 0 ? (
            <div className="text-[var(--text2)]">No events captured</div>
          ) : (
            events.slice(0, 20).map((event, index) => (
              <button
                key={`${event.ts}-${index}`}
                onClick={() => setSelectedEventIndex(index)}
                className={`w-full text-left flex justify-between gap-3 border-b border-[var(--border)] py-1 ${
                  selectedEventIndex === index ? "bg-[var(--bg2)]" : ""
                }`}
              >
                <span className="text-[var(--text1)]">{event.type}</span>
                <span className="text-[var(--text0)] flex-1">{event.location}</span>
              </button>
            ))
          )}
        </div>
        </div>

        <div className="bg-[var(--bg1)] border border-[var(--border2)] p-3">
          <div className="tracy-label-caps mb-2">spike correlation</div>
          {!selectedEvent ? (
            <div className="text-[11px] text-[var(--text2)]">Select an event to inspect the nearest hotspot snapshot.</div>
          ) : !correlatedSnapshot ? (
            <div className="text-[11px] text-[var(--text2)]">No hotspot snapshots available for this session.</div>
          ) : (
            <div className="space-y-3 text-[11px]">
              <MetaRow label="event" value={`${selectedEvent.type} | ${selectedEvent.location}`} />
              <MetaRow label="snapshot ts" value={correlatedSnapshot.ts.toString()} />
              <CorrelationSection title="dominant functions" rows={correlatedSnapshot.top_functions} />
              <CorrelationSection title="dominant crates" rows={correlatedSnapshot.crate_rollups} />
              <CorrelationSection title="dominant modules" rows={correlatedSnapshot.module_rollups} />
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-[var(--bg1)] border border-[var(--border2)] p-3">
      <div className="tracy-label-caps mb-1">{label}</div>
      <div className="text-[13px] text-[var(--text0)]">{value}</div>
    </div>
  );
}

function MetaRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3">
      <span className="text-[var(--text1)]">{label}</span>
      <span className="text-[var(--text0)] truncate">{value}</span>
    </div>
  );
}

function CorrelationSection({ title, rows }: { title: string; rows: ProfileRollup[] }) {
  return (
    <div>
      <div className="tracy-label-caps mb-1">{title}</div>
      <div className="space-y-1">
        {rows.length === 0 ? (
          <div className="text-[var(--text2)]">none</div>
        ) : (
          rows.slice(0, 3).map((row) => (
            <div key={row.name} className="flex justify-between gap-3">
              <span className="text-[var(--text0)] truncate">{row.name}</span>
              <span className="text-[var(--text1)]">{formatPct(row.total_pct || 0)}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
