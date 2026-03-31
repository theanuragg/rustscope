import type { FrameLayer, ChartType } from "@/types/profiler";

// ─── Flame palette (warm) ────────────────────────────────────────────────────
const FLAME: Record<FrameLayer, { bg: string; text: string }> = {
  kernel:  { bg: "#b83000", text: "rgba(255,220,200,0.95)" },
  std:     { bg: "#e05500", text: "rgba(255,230,190,0.9)"  },
  runtime: { bg: "#d04000", text: "rgba(255,225,185,0.9)"  },
  crate:   { bg: "#e89000", text: "#2a1400"                },
  dep:     { bg: "#b07840", text: "#1e1000"                },
  alloc:   { bg: "#4a8840", text: "rgba(210,240,200,0.9)"  },
  unknown: { bg: "#787060", text: "rgba(230,225,215,0.8)"  },
};

// ─── Icicle palette (cool blue) ──────────────────────────────────────────────
const ICICLE: Record<FrameLayer, { bg: string; text: string }> = {
  kernel:  { bg: "#0c3870", text: "rgba(200,225,255,0.95)" },
  std:     { bg: "#1458a8", text: "rgba(210,232,255,0.92)" },
  runtime: { bg: "#1870c8", text: "rgba(215,235,255,0.92)" },
  crate:   { bg: "#3090e8", text: "rgba(240,250,255,0.95)" },
  dep:     { bg: "#5aaae0", text: "rgba(240,250,255,0.9)"  },
  alloc:   { bg: "#3a8a5a", text: "rgba(210,245,225,0.9)"  },
  unknown: { bg: "#607898", text: "rgba(220,235,255,0.8)"  },
};

export function frameColor(
  layer: FrameLayer,
  chartType: ChartType,
  opts: { isSearchHit?: boolean; isSearchActive?: boolean } = {}
): { bg: string; text: string } {
  if (opts.isSearchActive) {
    if (opts.isSearchHit) {
      return { bg: "#f5c400", text: "#1a1000" };
    }
    // Dim non-matches heavily
    return { bg: chartType === "icicle" ? "#c8d8ec" : "#e8d8c8", text: "transparent" };
  }

  return chartType === "icicle" ? ICICLE[layer] : FLAME[layer];
}

/** UI accent colour that matches the current chart type */
export function accentColor(chartType: ChartType): string {
  return chartType === "icicle" ? "#1560b0" : "#d04800";
}

/** Legend entries for the current chart type */
export function legendEntries(chartType: ChartType): Array<{
  bg: string; layer: FrameLayer; label: string
}> {
  const entries = [
    { layer: "kernel",  label: "kernel / syscall" },
    { layer: "std",     label: "std / core / alloc" },
    { layer: "runtime", label: "async runtime (tokio…)" },
    { layer: "crate",   label: "your crate" },
    { layer: "dep",     label: "dependencies" },
    { layer: "alloc",   label: "allocator" },
    { layer: "unknown", label: "unknown / inlined" },
  ].map((e) => ({ ...e, ...frameColor(e.layer as FrameLayer, chartType) }));

  return entries as Array<{ bg: string; layer: FrameLayer; label: string }>;
}
