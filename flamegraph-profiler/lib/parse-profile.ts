import { z } from "zod";
import { demangleShort, extractCrate } from "./demangle";
import type {
  ProfileData,
  ProfileFrame,
  ProfileMeta,
  FrameLayer,
} from "@/types/profiler";

// ─── Zod schemas (v3 compatible) ───────────────────────────────────────────

const MetaSchema = z.object({
  project: z.string().optional(),
  duration_sec: z.number().optional(),
  start_ts: z.number().optional(),
  end_ts: z.number().optional(),
  rustscope_version: z.string().optional(),
  target_binary: z.string().optional(),
  host_os: z.string().optional(),
  cpu_cores: z.number().optional(),
}).optional();

const HostSchema = z.object({
  os: z.string().optional(),
  arch: z.string().optional(),
  cpu_logical_cores: z.number().optional(),
  rustc_version: z.string().optional(),
  build_profile: z.string().optional(),
}).optional();

const TimingSchema = z.object({
  total_ns: z.number().optional(),
  self_ns: z.number().optional(),
  avg_ns: z.number().optional(),
  min_ns: z.number().optional(),
  max_ns: z.number().optional(),
  p50_ns: z.number().optional(),
  p95_ns: z.number().optional(),
  p99_ns: z.number().optional(),
  pct_of_session: z.number().optional(),
  mean_ns: z.number().optional(),
  stddev_ns: z.number().optional(),
});

const MemorySchema = z.object({
  total_alloc_bytes: z.number().optional(),
  total_dealloc_bytes: z.number().optional(),
  peak_delta_bytes: z.number().optional(),
  alloc_count: z.number().optional(),
  dealloc_count: z.number().optional(),
  net_retained_bytes: z.number().optional(),
  mean_alloc_per_call: z.number().optional(),
  alloc_op_count: z.number().optional(),
}).optional();

const CpuCountersSchema = z.object({
  cpu_cycles: z.number().optional(),
  instructions: z.number().optional(),
  ipc: z.number().optional(),
  cache_miss_rate: z.number().optional(),
  branch_miss_rate: z.number().optional(),
}).optional();

const FunctionSchema = z.object({
  name: z.string(),
  module_path: z.string().optional(),
  file: z.string().optional(),
  line: z.number().optional(),
  call_count: z.number().optional(),
  max_recursion_depth: z.number().optional(),
  timing: TimingSchema.optional(),
  memory: MemorySchema.optional(),
  cpu: CpuCountersSchema.optional(),
  // CLI fields
  self_pct: z.number().optional(),
  total_pct: z.number().optional(),
  calls: z.number().optional(),
  depth: z.number().optional(),
  x: z.number().optional(),
  w: z.number().optional(),
});

const CallTreeSchema: z.ZodType<any> = z.lazy(() => z.object({
  name: z.string(),
  call_count: z.number().optional(),
  total_ns: z.number().optional(),
  duration_ns: z.number().optional(),
  alloc_bytes: z.number().optional(),
  cpu_cycles: z.number().optional(),
  file: z.string().optional(),
  line: z.number().optional(),
  children: z.array(CallTreeSchema).optional(),
}));

const SessionMemorySchema = z.object({
  total_alloc_bytes: z.number().optional(),
  total_dealloc_bytes: z.number().optional(),
  peak_rss_mb: z.number().optional(),
  peak_heap_bytes: z.number().optional(),
  final_heap_bytes: z.number().optional(),
  total_alloc_ops: z.number().optional(),
}).optional();

const UploadedProfileSchema = z.object({
  schema_version: z.number().optional(),
  started_at_unix_secs: z.number().optional(),
  session_duration_ns: z.number().optional(),
  host: HostSchema,
  meta: MetaSchema,
  functions: z.array(FunctionSchema).optional(),
  call_trees: z.array(CallTreeSchema).optional(),
  session_memory: SessionMemorySchema,
  // CLI legacy
  samples: z.array(z.any()).optional(),
  summary: z.any().optional(),
  memory_events: z.array(z.any()).optional(),
});

// ─── Formatting Utils ──────────────────────────────────────────────────────

export const formatNs = (ns: number): string => {
  const abs = Math.abs(ns);
  const sign = ns < 0 ? "-" : "";
  if (abs < 1000) return `${sign}${abs} ns`;
  if (abs < 1_000_000) return `${sign}${(abs / 1000).toFixed(2)} µs`;
  if (abs < 1_000_000_000) return `${sign}${(abs / 1_000_000).toFixed(3)} ms`;
  return `${sign}${(abs / 1_000_000_000).toFixed(3)} s`;
};

export const formatBytes = (b: number): string => {
  const abs = Math.abs(b);
  const sign = b < 0 ? "-" : "";
  if (abs < 1024) return `${sign}${abs} B`;
  if (abs < 1_048_576) return `${sign}${(abs / 1024).toFixed(1)} KB`;
  return `${sign}${(abs / 1_048_576).toFixed(2)} MB`;
};

export const formatPct = (f: number): string => `${f.toFixed(1)}%`;

export const formatCount = (n: number): string => n.toLocaleString();

export const heatColor = (pct: number): string => {
  if (pct >= 70) return "var(--hot)";
  if (pct >= 30) return "var(--warm)";
  return "var(--cool)";
};

// ─── Layer classification ────────────────────────────────────────────────────

const KERNEL_PATTERNS = [/^kernel/, /^sys_/, /^\[kernel/, /^kthread/, /^irq/];
const STD_PATTERNS    = [/^std::/, /^core::/, /^alloc::/, /^__rust_/];
const RUNTIME_PATTERNS= [/^tokio::/, /^async_std::/, /^rayon::/, /^futures::/, /^hyper::runtime/];
const ALLOC_PATTERNS  = [/^__rdl_/, /^__rg_/, /jemalloc/, /mimalloc/, /tcmalloc/, /GlobalAlloc/];

function classifyLayer(symbol: string): FrameLayer {
  const s = symbol.toLowerCase();
  if (KERNEL_PATTERNS.some((p) => p.test(s))) return "kernel";
  if (ALLOC_PATTERNS.some((p) => p.test(s)))  return "alloc";
  if (STD_PATTERNS.some((p) => p.test(s)))     return "std";
  if (RUNTIME_PATTERNS.some((p) => p.test(s))) return "runtime";
  if (/::h[0-9a-f]{8,16}$/.test(symbol))       return "crate";
  if (symbol.includes("::"))                    return "dep";
  return "unknown";
}

// ─── Data Transformation ────────────────────────────────────────────────────

export interface ParseResult {
  error?: string;
  ok: boolean;
  data?: ProfileData;
}

function parseCallTree(
  nodes: any[],
  durationNs: number,
  depth: number = 0,
  startX: number = 0
): ProfileFrame[] {
  let frames: ProfileFrame[] = [];
  let currentX = startX;

  for (const node of nodes) {
    const totalNs = node.duration_ns || node.total_ns || 0;
    const totalPct = durationNs > 0 ? (totalNs / durationNs) * 100 : 0;
    
    const childrenTotalNs = (node.children || []).reduce((acc: number, c: any) => acc + (c.duration_ns || c.total_ns || 0), 0);
    const selfNs = Math.max(0, totalNs - childrenTotalNs);
    const selfPct = durationNs > 0 ? (selfNs / durationNs) * 100 : 0;

    frames.push({
      name: node.name,
      displayName: demangleShort(node.name),
      crate: extractCrate(node.name),
      layer: classifyLayer(node.name),
      totalNs,
      selfNs,
      totalPct,
      selfPct,
      depth,
      x: currentX,
      w: totalPct,
      samples: node.call_count || 0,
      allocBytes: node.alloc_bytes,
      file: node.file,
      line: node.line,
    });

    if (node.children && node.children.length > 0) {
      frames = frames.concat(parseCallTree(node.children, durationNs, depth + 1, currentX));
    }

    currentX += totalPct;
  }

  return frames;
}

export function parseSession(json: unknown): ParseResult {
  const parsed = UploadedProfileSchema.safeParse(json);
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0].message };
  }

  const raw = parsed.data as any;
  const durationNs = raw.session_duration_ns || (raw.meta?.duration_sec ? raw.meta.duration_sec * 1e9 : 1e9);

  const meta: ProfileMeta = {
    name: raw.meta?.project || raw.host?.os || "Session",
    rustVersion: raw.host?.rustc_version || raw.meta?.rustscope_version,
    durationNs,
    tool: "rustscope",
    capturedAt: raw.started_at_unix_secs ? new Date(raw.started_at_unix_secs * 1000).toISOString() : undefined,
    totalSamples: 0
  };

  let cpu: ProfileFrame[] = [];
  if (raw.call_trees) {
    cpu = parseCallTree(raw.call_trees, durationNs);
  } else if (raw.functions) {
    let curX = 0;
    cpu = raw.functions.map((f: any) => {
      const totalPct = f.timing?.pct_of_session || f.total_pct || 0;
      const frame = {
        name: f.name,
        displayName: demangleShort(f.name),
        crate: extractCrate(f.name),
        layer: classifyLayer(f.name),
        totalNs: f.timing?.total_ns || (totalPct / 100) * durationNs,
        selfNs: f.timing?.self_ns || 0,
        totalPct,
        selfPct: f.timing?.pct_of_session || f.self_pct || 0,
        depth: f.depth || 0,
        x: f.x ?? curX,
        w: f.w ?? totalPct,
        samples: f.call_count || f.calls || 0,
        file: f.file,
        line: f.line,
      };
      if (f.x === undefined) curX += totalPct;
      return frame;
    });
  }

  return {
    ok: true,
    data: {
      meta,
      cpu,
      functions: raw.functions || [],
      samples: raw.samples,
      events: raw.memory_events,
      raw, // Keep raw for detailed views
    },
  };
}
