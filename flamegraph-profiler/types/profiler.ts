
/** A single frame in a call stack */
export interface ProfileFrame {
  /** Mangled or raw symbol name */
  name: string;
  /** Demangled display name (computed) */
  displayName: string;
  /** Crate/module origin */
  crate: string;
  /** Layer classification */
  layer: FrameLayer;
  /** Self time in nanoseconds */
  selfNs: number;
  /** Total (inclusive) time in nanoseconds */
  totalNs: number;
  /** Self time as % of profile total */
  selfPct: number;
  /** Total time as % of profile total */
  totalPct: number;
  /** Call stack depth (0 = root) */
  depth: number;
  /** X offset as % of parent width */
  x: number;
  /** Width as % of total profile */
  w: number;
  /** Number of times this frame was sampled */
  samples: number;
  /** Heap bytes allocated in this frame (optional) */
  allocBytes?: number;
  /** Number of Arc clones in this frame (optional) */
  arcClones?: number;
  /** Async poll count (optional) */
  pollCount?: number;
  /** Whether this is a future poll frame */
  isAsync?: boolean;
  /** Source file path */
  file?: string;
  /** Source line number */
  line?: number;
}

export type FrameLayer =
  | "kernel"
  | "std"
  | "runtime"    // tokio, async-std, rayon
  | "crate"      // your crate
  | "dep"        // third-party crates
  | "alloc"      // allocator frames
  | "unknown";

/** Profile mode — what metric is being visualised */
export type ProfileMode = "timeline" | "cpu" | "memory" | "threads" | "allocator" | "compare" | "alloc" | "offcpu";

/** Chart layout */
export type ChartType = "flame" | "icicle";

/** The full uploaded/parsed profile */
export interface ProfileData {
  /** Profile metadata */
  meta: ProfileMeta;
  /** Frames for cpu mode */
  cpu: ProfileFrame[];
  /** Detailed function list (Stats view) */
  functions?: ProfileFunction[];
  /** Crate-level hotspot rollups */
  crateRollups?: ProfileRollup[];
  /** Module-level hotspot rollups */
  moduleRollups?: ProfileRollup[];
  /** Frames for allocation mode (optional) */
  alloc?: ProfileFrame[];
  /** Frames for off-cpu mode (optional) */
  offcpu?: ProfileFrame[];
  /** Time-series samples (for RustScope native format) */
  samples?: Array<{
    ts: number;
    cpu_pct: number;
    heap_mb: number;
    threads: number;
    open_fds: number;
    syscalls_per_sec: number;
  }>;
  /** Significant events (spikes, etc) */
  events?: Array<{
    ts: number;
    type: string;
    location: string;
    size_bytes?: number;
  }>;
  /** Time-indexed hotspot snapshots for session correlation */
  hotspotSnapshots?: ProfileHotspotSnapshot[];
  /** Raw uploaded JSON */
  raw?: any;
}

export interface ProfileRollup {
  name: string;
  total_pct: number;
  self_pct: number;
  calls: number;
  function_count: number;
}

export interface ProfileHotspotSnapshot {
  ts: number;
  top_functions: ProfileRollup[];
  crate_rollups: ProfileRollup[];
  module_rollups: ProfileRollup[];
}

export interface ProfileFunction {
  name: string;
  module_path?: string;
  file?: string;
  line?: number;
  call_count?: number;
  timing?: {
    total_ns: number;
    self_ns: number;
    avg_ns?: number;
    min_ns?: number;
    max_ns?: number;
    p50_ns?: number;
    p95_ns?: number;
    p99_ns?: number;
    pct_of_session?: number;
  };
  memory?: {
    total_alloc_bytes?: number;
    total_dealloc_bytes?: number;
    peak_delta_bytes?: number;
    alloc_count?: number;
    dealloc_count?: number;
    net_retained_bytes?: number;
    mean_alloc_per_call?: number;
  };
  // V4 fields
  drop_timing?: {
    drop_ns: number;
    drop_call_count: number;
    drop_chain_depth: number;
    largest_drop_type?: string;
  };
  clone_tracking?: {
    clone_count: number;
    clone_bytes: number;
  };
  llvm_hints?: {
    bounds_checks_remaining: number;
    vectorized: boolean;
  };
  generics?: {
    trait_object_calls: number;
  };
  sync_contention?: {
    mutex_wait_ns: number;
    mutex_contention_count: number;
  };
  async_task?: {
    poll_count: number;
    time_pending_ns: number;
    yield_count: number;
    wakeup_latency_ns?: {
      min_ns: number;
      p50_ns: number;
      p99_ns: number;
    };
  };
}

export interface ProfileMeta {
  /** Binary/process name */
  name: string;
  /** Rust toolchain version if known */
  rustVersion?: string;
  /** Profile duration in nanoseconds */
  durationNs: number;
  /** Sample frequency in Hz */
  sampleHz?: number;
  /** Total samples taken */
  totalSamples: number;
  /** Peak heap in bytes */
  peakHeapBytes?: number;
  /** Profile tool that generated this */
  tool?: string;
  /** ISO timestamp */
  capturedAt?: string;
}

// ─── Profiler state ────────────────────────────────────────────────────────

export interface ProfilerState {
  mode: ProfileMode;
  chartType: ChartType;
  /** Current zoom stack — each entry is a frame we've zoomed into */
  zoomStack: ProfileFrame[];
  /** Search query */
  search: string;
  /** Active layer filters (empty = show all) */
  layerFilters: Set<FrameLayer>;
  /** Whether to collapse identical adjacent frames */
  collapseInlines: boolean;
  /** Min width % to render a frame (performance gate) */
  minWidthPct: number;
  /** Hovered frame */
  hoveredFrame: ProfileFrame | null;
}

export type ProfilerAction =
  | { type: "SET_MODE"; mode: ProfileMode }
  | { type: "SET_CHART_TYPE"; chartType: ChartType }
  | { type: "ZOOM_IN"; frame: ProfileFrame }
  | { type: "ZOOM_TO"; depth: number }
  | { type: "ZOOM_RESET" }
  | { type: "SET_SEARCH"; search: string }
  | { type: "TOGGLE_LAYER"; layer: FrameLayer }
  | { type: "RESET_LAYERS" }
  | { type: "SET_HOVERED"; frame: ProfileFrame | null }
  | { type: "TOGGLE_COLLAPSE_INLINES" };

// ─── JSON upload schema (flexible — supports multiple tools) ───────────────

/**
 * Accepted JSON format. Supports:
 * - Our custom format (preferred)
 * - cargo-flamegraph / inferno stackcollapse format
 * - samply / Firefox Profiler subset
 * - pprof JSON export
 */
export interface UploadedProfile {
  /** Format discriminator */
  format?: "flamegraph-profiler" | "inferno" | "samply" | "pprof" | "custom";
  meta?: Partial<ProfileMeta>;
  /** Direct frame arrays (our format) */
  frames?: UploadedFrame[];
  cpu?: UploadedFrame[];
  alloc?: UploadedFrame[];
  offcpu?: UploadedFrame[];
  /** inferno/cargo-flamegraph: flat stacks */
  stacks?: InflatedStack[];
  /** samply: sample array */
  samples?: SamplySample[];
}

export interface UploadedFrame {
  name: string;
  self_ns?: number;
  self_pct?: number;
  total_ns?: number;
  total_pct?: number;
  depth: number;
  x: number;
  w: number;
  samples?: number;
  alloc_bytes?: number;
  arc_clones?: number;
  poll_count?: number;
  is_async?: boolean;
}

export interface InflatedStack {
  /** semicolon-separated stack frames, deepest last */
  stack: string;
  /** sample count */
  count: number;
}

export interface SamplySample {
  stack: string[];
  weight: number;
}

// ─── Computed layout ───────────────────────────────────────────────────────

export interface LayoutFrame extends ProfileFrame {
  /** Pixel X position */
  px: number;
  /** Pixel width */
  pw: number;
  /** Pixel Y position */
  py: number;
  /** Row height */
  ph: number;
  /** Fill color */
  color: string;
  /** Text color */
  textColor: string;
  /** Is this frame visible given current zoom/filter? */
  visible: boolean;
  /** Is this a search hit? */
  searchHit: boolean;
}

// ─── Insights ─────────────────────────────────────────────────────────────

export interface Insight {
  severity: "critical" | "warn" | "info";
  title: string;
  body: string;
  /** Optional frame name to highlight */
  relatedFrame?: string;
}
