"use client";

import React, { useCallback } from "react";
import type { ProfilerState, ProfilerAction, ProfileData, FrameLayer, ProfileMode, ChartType } from "@/types/profiler";
import { legendEntries } from "@/lib/colors";
import clsx from "clsx";

interface Props {
  state: ProfilerState;
  data: ProfileData;
  dispatch: React.Dispatch<ProfilerAction>;
}

const MODES: Array<{ id: ProfileMode; label: string; desc: string }> = [
  { id: "cpu",    label: "CPU time",    desc: "On-CPU samples" },
  { id: "alloc",  label: "Allocations", desc: "Heap alloc trace" },
  { id: "offcpu", label: "Off-CPU",     desc: "Blocked / waiting" },
];

export function Toolbar({ state, data, dispatch }: Props) {
  const setMode = useCallback(
    (mode: ProfileMode) => dispatch({ type: "SET_MODE", mode }),
    [dispatch]
  );
  const setChartType = useCallback(
    (chartType: ChartType) => dispatch({ type: "SET_CHART_TYPE", chartType }),
    [dispatch]
  );
  const setSearch = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) =>
      dispatch({ type: "SET_SEARCH", search: e.target.value }),
    [dispatch]
  );
  const resetZoom = useCallback(
    () => dispatch({ type: "ZOOM_RESET" }),
    [dispatch]
  );
  const toggleLayer = useCallback(
    (layer: FrameLayer) => dispatch({ type: "TOGGLE_LAYER", layer }),
    [dispatch]
  );
  const resetLayers = useCallback(
    () => dispatch({ type: "RESET_LAYERS" }),
    [dispatch]
  );

  const isIcicle = state.chartType === "icicle";
  const accent = isIcicle ? "ice" : "rust";

  // Disable alloc/offcpu tabs if data not present
  const hasAlloc  = (data.alloc?.length ?? 0) > 0;
  const hasOffcpu = (data.offcpu?.length ?? 0) > 0;

  return (
    <div className="space-y-3 mb-4">
      {/* Row 1: Mode + Chart type */}
      <div className="flex items-center gap-3 flex-wrap">
        {/* Mode tabs */}
        <div
          className={clsx(
            "flex gap-1 p-1 rounded-lg border",
            isIcicle ? "bg-ice-50 border-ice-100" : "bg-stone-100 border-stone-200"
          )}
        >
          {MODES.map((m) => {
            const disabled = m.id === "alloc" ? !hasAlloc : m.id === "offcpu" ? !hasOffcpu : false;
            return (
              <button
                key={m.id}
                disabled={disabled}
                onClick={() => setMode(m.id)}
                title={m.desc}
                className={clsx(
                  "font-mono text-[11px] font-medium px-3 py-1.5 rounded-md transition-all",
                  "disabled:opacity-30 disabled:cursor-not-allowed",
                  state.mode === m.id
                    ? isIcicle
                      ? "bg-white text-ice-700 border border-ice-200 shadow-sm"
                      : "bg-white text-orange-700 border border-stone-200 shadow-sm"
                    : "text-stone-500 hover:text-stone-700"
                )}
              >
                {m.label}
              </button>
            );
          })}
        </div>

        {/* Chart type */}
        <div className="flex gap-1 p-1 rounded-lg bg-stone-100 border border-stone-200">
          <ChartTypeBtn
            id="flame"
            label="🔥 Flame"
            active={!isIcicle}
            onClick={() => setChartType("flame")}
            activeClass="text-orange-700"
          />
          <ChartTypeBtn
            id="icicle"
            label="🧊 Icicle"
            active={isIcicle}
            onClick={() => setChartType("icicle")}
            activeClass="text-blue-700"
          />
        </div>

        {/* Zoom reset */}
        {state.zoomStack.length > 0 && (
          <button
            onClick={resetZoom}
            className="font-mono text-[11px] text-stone-500 hover:text-stone-700 border border-stone-200 rounded-lg px-3 py-1.5 bg-white transition-colors"
          >
            ↺ reset zoom
          </button>
        )}
      </div>

      {/* Row 2: Search */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1 max-w-sm">
          <svg
            className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-stone-400"
            fill="none" stroke="currentColor" strokeWidth={2}
            viewBox="0 0 24 24"
          >
            <circle cx="11" cy="11" r="8" /><path d="m21 21-4.35-4.35" />
          </svg>
          <input
            type="text"
            value={state.search}
            onChange={setSearch}
            placeholder="search symbols, crates…"
            className={clsx(
              "w-full font-mono text-[12px] bg-white border rounded-lg pl-8 pr-3 py-2 outline-none transition-colors placeholder:text-stone-300",
              isIcicle
                ? "border-stone-200 focus:border-blue-400"
                : "border-stone-200 focus:border-orange-400"
            )}
          />
          {state.search && (
            <button
              onClick={() => dispatch({ type: "SET_SEARCH", search: "" })}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-stone-400 hover:text-stone-600 font-mono text-xs"
            >
              ✕
            </button>
          )}
        </div>

        {/* Layer filters */}
        <div className="flex items-center gap-1 flex-wrap">
          {legendEntries(state.chartType).map((e) => (
            <LayerPill
              key={e.layer}
              layer={e.layer}
              label={e.label}
              color={e.bg}
              active={state.layerFilters.size === 0 || state.layerFilters.has(e.layer)}
              onClick={() => toggleLayer(e.layer)}
            />
          ))}
          {state.layerFilters.size > 0 && (
            <button
              onClick={resetLayers}
              className="font-mono text-[10px] text-stone-400 hover:text-stone-600 px-2 py-1"
            >
              clear
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function ChartTypeBtn({
  label, active, onClick, activeClass,
}: {
  id: string; label: string; active: boolean; onClick: () => void; activeClass: string;
}) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        "font-mono text-[11px] font-medium px-3 py-1.5 rounded-md transition-all",
        active
          ? `bg-white ${activeClass} border border-stone-200 shadow-sm`
          : "text-stone-500 hover:text-stone-700"
      )}
    >
      {label}
    </button>
  );
}

function LayerPill({
  layer, label, color, active, onClick,
}: {
  layer: FrameLayer; label: string; color: string; active: boolean; onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      className={clsx(
        "flex items-center gap-1.5 font-mono text-[10px] px-2 py-1 rounded-md border transition-all",
        active
          ? "bg-white border-stone-200 text-stone-600"
          : "bg-transparent border-stone-100 text-stone-300"
      )}
    >
      <span
        className="w-2 h-2 rounded-sm flex-shrink-0"
        style={{ background: active ? color : "#d4d0c8" }}
      />
      {layer}
    </button>
  );
}
