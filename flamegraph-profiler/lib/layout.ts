import type { ProfileFrame, ProfilerState, LayoutFrame } from "@/types/profiler";
import { frameColor } from "./colors";

export interface LayoutOptions {
  width: number;
  rowHeight: number;
  gap: number;
  state: ProfilerState;
}

/**
 * Compute pixel positions for all visible frames.
 *
 * This is pure and deterministic — safe to memoise with useMemo.
 * Runs in O(n) over the frame array.
 */
export function computeLayout(
  frames: ProfileFrame[],
  opts: LayoutOptions
): { layout: LayoutFrame[]; totalHeight: number } {
  const { width, rowHeight, gap, state } = opts;
  const { zoomStack, chartType, search, layerFilters } = state;

  // Determine zoom window
  let xMin = 0;
  let xMax = 100;
  let minDepth = 0;

  if (zoomStack.length > 0) {
    const z = zoomStack[zoomStack.length - 1];
    xMin = z.x;
    xMax = z.x + z.w;
    minDepth = z.depth;
  }

  const xScale = 100 / (xMax - xMin);
  const sl = search.trim().toLowerCase();
  const isSearchActive = sl.length > 0;

  // Filter frames to zoom window
  const visible = frames.filter((f) => {
    if (f.depth < minDepth) return false;
    const inWindow = f.x >= xMin - 0.05 && f.x + f.w <= xMax + 0.05;
    if (!inWindow) return false;
    if (layerFilters.size > 0 && !layerFilters.has(f.layer)) return false;
    return true;
  });

  if (frames.length > 0 && visible.length === 0) {
    console.warn("No visible frames in current window", { xMin, xMax, minDepth, framesCount: frames.length });
  }

  const maxDepth = visible.reduce((m, f) => Math.max(m, f.depth), 0);
  const rows = maxDepth - minDepth + 1;
  const totalHeight = rows * (rowHeight + gap) + gap;

  const layout: LayoutFrame[] = [];

  for (const f of visible) {
    const relX = (f.x - xMin) * xScale;
    const relW = f.w * xScale;
    const pw = (relW / 100) * width;

    // Skip frames narrower than 1px — invisible and wastes canvas calls
    if (pw < 1) continue;

    const px = (relX / 100) * width;
    const depthOffset = f.depth - minDepth;

    const py =
      chartType === "icicle"
        ? depthOffset * (rowHeight + gap) + gap
        : totalHeight - (depthOffset + 1) * (rowHeight + gap);

    const isSearchHit = isSearchActive && f.displayName.toLowerCase().includes(sl);
    const colors = frameColor(f.layer, chartType, {
      isSearchHit,
      isSearchActive,
    });

    layout.push({
      ...f,
      px,
      pw,
      py,
      ph: rowHeight,
      color: colors.bg,
      textColor: colors.text,
      visible: true,
      searchHit: isSearchHit,
    });
  }

  return { layout, totalHeight };
}

/** How many chars fit in a given pixel width at our font size */
export function labelChars(pixelWidth: number, fontSize = 11): number {
  // JetBrains Mono is roughly 0.6× em wide per char
  return Math.floor(pixelWidth / (fontSize * 0.62));
}

/** Truncate label to fit */
export function truncateLabel(name: string, pixelWidth: number): string {
  if (pixelWidth < 20) return "";
  const chars = labelChars(pixelWidth);
  if (chars < 3) return "";
  if (name.length <= chars) return name;
  return name.slice(0, chars - 1) + "…";
}
