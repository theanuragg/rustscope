//! JSON output schema for RustScope.
//!
//! Every field that could ever be absent (e.g. CPU counters on non-Linux)
//! is wrapped in `Option<T>` so the JSON stays forward-compatible.

use serde::{Deserialize, Serialize};

/// Root object written to `profile.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSession {
    /// Schema version — bump when the shape changes.
    pub schema_version: u32,
    /// Unix epoch seconds when `Profiler::init()` was called.
    pub started_at_unix_secs: u64,
    /// Total wall-clock duration of the profiled session (nanoseconds).
    pub session_duration_ns: u64,
    /// Host info captured at init time.
    pub host: HostInfo,
    /// Per-function profile records (one per unique instrumented function).
    pub functions: Vec<FunctionRecord>,
    /// Statistical benchmark records (from `#[benchmark]` / `run_benchmark`).
    pub benchmarks: Vec<BenchmarkRecord>,
    /// Root nodes of observed call trees (depth-first, built from call stack).
    pub call_trees: Vec<CallTreeNode>,
    /// Session-level memory summary.
    pub session_memory: SessionMemory,
    /// Async span records (Phase 2). None if async-profiling feature disabled
    /// or RustScopeLayer was not installed as a tracing subscriber.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async_spans: Option<serde_json::Value>,
    /// Per-thread profiling breakdown (Phase 4). None if ThreadProfiler::enable()
    /// was not called before profiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_report: Option<serde_json::Value>,
    /// Session metadata (git commit, tags, CI info, custom KV).
    /// None if no metadata was attached via Profiler::tag() / Profiler::meta().
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_meta: Option<serde_json::Value>,
    /// Outlier detection summary + budget violation summary.
    /// None if no outliers/violations were detected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlier_summary: Option<serde_json::Value>,
    /// Timeline summary (event count, slowest call, etc.).
    /// None if timeline was not enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_summary: Option<serde_json::Value>,
    /// Lock contention metrics collected during the session.
    /// Empty when lock profiling is disabled or no locks were observed.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub locks: Vec<LockRecord>,
    /// Async task/span metrics (Tokio/async-std via tracing).
    /// Empty when async profiling is disabled or no tasks were observed.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub async_tasks: Vec<AsyncTaskRecord>,
}

/// Static information about the machine and build.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub cpu_logical_cores: u32,
    /// Rust toolchain version string (captured at compile time).
    pub rustc_version: String,
    /// Build profile ("debug" | "release").
    pub build_profile: String,
}

/// All metrics collected for one instrumented function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionRecord {
    // ── identity ──────────────────────────────────────────────────
    pub name: String,
    pub module_path: String,
    pub file: String,
    pub line: u32,

    // ── invocation counts ─────────────────────────────────────────
    /// Total number of times the function was called.
    pub call_count: u64,
    /// Maximum recursion depth observed (1 = no recursion).
    pub max_recursion_depth: u32,

    // ── timing ────────────────────────────────────────────────────
    pub timing: TimingStats,

    // ── memory ────────────────────────────────────────────────────
    /// `None` if `TrackingAllocator` is not installed as `#[global_allocator]`.
    pub memory: Option<MemoryMetrics>,

    // ── stack ─────────────────────────────────────────────────────
    pub stack: StackMetrics,

    // ── CPU hardware counters ─────────────────────────────────────
    /// `None` on non-Linux or when `hw-counters` feature is disabled.
    pub cpu: Option<CpuCounters>,
    /// Number of calls flagged as outliers (> 3σ above mean). 0 if none.
    #[serde(skip_serializing_if = "crate::output::schema::is_zero")]
    pub outlier_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingStats {
    pub total_ns: u64,
    pub self_ns: u64,
    pub avg_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub pct_of_session: f64,
    pub mean_ns: f64,
    pub stddev_ns: f64,
    pub p50_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryMetrics {
    pub total_alloc_bytes: u64,
    pub total_dealloc_bytes: u64,
    pub peak_delta_bytes: i64,
    pub alloc_count: u64,
    pub dealloc_count: u64,
    pub net_retained_bytes: i64,
    pub mean_alloc_per_call: f64,
    pub alloc_op_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StackMetrics {
    pub max_depth: u32,
    pub avg_depth: f64,
    pub frame_size_bytes: Option<u64>,
    pub max_call_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuCounters {
    pub cpu_cycles: u64,
    pub instructions: u64,
    pub cache_misses: u64,
    pub cache_references: u64,
    pub branch_misses: u64,
    pub branch_instructions: u64,
    pub ipc: f64,
    pub cache_miss_rate: f64,
    pub branch_miss_rate: f64,
    pub l1_dcache_loads: u64,
    pub l1_dcache_load_misses: u64,
    pub llc_loads: u64,
    pub llc_load_misses: u64,
    pub context_switches: u64,
    pub page_faults: u64,
    pub cpu_migrations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    pub name: String,
    pub iterations: u64,
    pub total_ns: u64,
    pub avg_ns: u64,
    pub memory: Option<MemoryMetrics>,
    pub file: String,
    pub line: u32,
    pub warmup_iterations: u64,
    pub throughput_per_sec: f64,
    pub timing: TimingStats,
    pub cpu: Option<CpuCounters>,
    pub raw_samples_ns: Option<Vec<u64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTreeNode {
    pub name: String,
    pub call_count: u64,
    pub total_ns: u64,
    pub children: Vec<CallTreeNode>,
    pub file: String,
    pub line: u32,
    pub duration_ns: u64,
    pub alloc_bytes: Option<u64>,
    pub cpu_cycles: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMemory {
    pub total_alloc_bytes: u64,
    pub total_dealloc_bytes: u64,
    pub peak_rss_mb: f64,
    pub peak_heap_bytes: u64,
    pub final_heap_bytes: u64,
    pub total_alloc_ops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockRecord {
    pub name: String,
    pub contention_count: u64,
    pub total_wait_ns: u64,
    pub max_wait_ns: u64,
    pub wait_ns: u64,
    pub hold_ns: u64,
    pub acquisitions: u64,
    pub contended_acquisitions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncTaskRecord {
    pub name: String,
    pub total_active_ns: u64,
    pub total_idle_ns: u64,
    pub poll_count: u64,
}

pub fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchema {
    pub meta: Meta,
    pub summary: Summary,
    pub samples: Vec<Sample>,
    pub functions: Vec<Function>,
    pub allocations: Allocations,
    pub memory_events: Vec<MemoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub project: String,
    pub duration_sec: u64,
    pub start_ts: u64,
    pub end_ts: u64,
    pub rustscope_version: String,
    pub target_binary: String,
    pub host_os: String,
    pub cpu_cores: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub cpu_avg_pct: f64,
    pub cpu_peak_pct: f64,
    pub heap_avg_mb: f64,
    pub heap_peak_mb: f64,
    pub thread_avg: u32,
    pub fd_peak: u32,
    pub total_allocations: u64,
    pub total_deallocations: u64,
    pub leaked_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub ts: u64,
    pub cpu_pct: f64,
    pub heap_mb: f64,
    pub threads: u32,
    pub open_fds: u32,
    pub syscalls_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub module: String,
    pub self_pct: f64,
    pub total_pct: f64,
    pub calls: u64,
    pub avg_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allocations {
    pub by_size: BySize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BySize {
    #[serde(rename = "0-64B")]
    pub range_0_64b: u64,
    #[serde(rename = "65-512B")]
    pub range_65_512b: u64,
    #[serde(rename = "513B-4KB")]
    pub range_513b_4kb: u64,
    #[serde(rename = "4KB-64KB")]
    pub range_4kb_64kb: u64,
    #[serde(rename = ">64KB")]
    pub range_gt_64kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvent {
    pub ts: u64,
    #[serde(rename = "type")]
    pub event_type: String, // "alloc" | "dealloc" | "spike"
    pub size_bytes: u64,
    pub location: String,
}
