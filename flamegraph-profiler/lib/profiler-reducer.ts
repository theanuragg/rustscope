import type { ProfilerState, ProfilerAction, ProfileMode, ChartType } from "@/types/profiler";

export const initialState: ProfilerState = {
  mode: "timeline",
  chartType: "flame",
  zoomStack: [],
  search: "",
  layerFilters: new Set(),
  collapseInlines: false,
  minWidthPct: 0.05,
  hoveredFrame: null,
};

export function profilerReducer(
  state: ProfilerState,
  action: ProfilerAction
): ProfilerState {
  switch (action.type) {
    case "SET_MODE":
      return {
        ...state,
        mode: action.mode,
        // Reset zoom when switching modes — frames are different
        zoomStack: [],
        hoveredFrame: null,
      };

    case "SET_CHART_TYPE":
      return { ...state, chartType: action.chartType };

    case "ZOOM_IN":
      return {
        ...state,
        zoomStack: [...state.zoomStack, action.frame],
        hoveredFrame: null,
      };

    case "ZOOM_TO": {
      const depth = action.depth;
      if (depth < 0) return { ...state, zoomStack: [], hoveredFrame: null };
      return {
        ...state,
        zoomStack: state.zoomStack.slice(0, depth + 1),
        hoveredFrame: null,
      };
    }

    case "ZOOM_RESET":
      return { ...state, zoomStack: [], hoveredFrame: null };

    case "SET_SEARCH":
      return { ...state, search: action.search };

    case "TOGGLE_LAYER": {
      const next = new Set(state.layerFilters);
      if (next.has(action.layer)) {
        next.delete(action.layer);
      } else {
        next.add(action.layer);
      }
      return { ...state, layerFilters: next };
    }

    case "RESET_LAYERS":
      return { ...state, layerFilters: new Set() };

    case "SET_HOVERED":
      return { ...state, hoveredFrame: action.frame };

    case "TOGGLE_COLLAPSE_INLINES":
      return { ...state, collapseInlines: !state.collapseInlines };

    default:
      return state;
  }
}
