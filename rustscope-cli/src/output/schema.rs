use serde::{Deserialize, Serialize};

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
