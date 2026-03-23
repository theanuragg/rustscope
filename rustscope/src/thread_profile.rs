//! # Thread-level profiling breakdown
//!
//! Tracks which thread executed each function call and aggregates
//! per-thread timing, so you can see:
//! - Thread contention (functions that run on many threads)
//! - Load imbalance (one thread doing all the work)
//! - Cross-thread latency (time between spawn and first execution)
//!
//! ## Usage
//!
//! ```rust
//! use rustscope::thread_profile::ThreadProfiler;
//!
//! fn main() {
//!     rustscope::Profiler::init();
//!     ThreadProfiler::enable();
//!
//!     // ... run multithreaded code with #[profile] annotations ...
//!
//!     rustscope::Profiler::save_json("profile.json").unwrap();
//!     ThreadProfiler::save_json("thread_profile.json").unwrap();
//! }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// ─── thread ID mapping ────────────────────────────────────────────────────────

static NEXT_THREAD_NUM: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static THREAD_NUM: u64 = NEXT_THREAD_NUM.fetch_add(1, Relaxed);
    static THREAD_NAME: String = {
        std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_owned()
    };
}

pub fn current_thread_num() -> u64 {
    THREAD_NUM.with(|n| *n)
}

pub fn current_thread_name() -> String {
    THREAD_NAME.with(|n| n.clone())
}

// ─── global thread profiler ───────────────────────────────────────────────────

struct ThreadEntry {
    thread_name: String,
    /// function_name → accumulated nanoseconds
    fn_time_ns: HashMap<String, u64>,
    fn_call_count: HashMap<String, u64>,
    fn_self_ns: HashMap<String, u64>,
    active_ns: u64,
    first_seen: Instant,
    last_seen: Instant,
}

impl ThreadEntry {
    fn new(name: String) -> Self {
        let now = Instant::now();
        Self {
            thread_name: name,
            fn_time_ns: HashMap::new(),
            fn_call_count: HashMap::new(),
            fn_self_ns: HashMap::new(),
            active_ns: 0,
            first_seen: now,
            last_seen: now,
        }
    }
}

static THREAD_DATA: Lazy<Mutex<HashMap<u64, ThreadEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Record one function call on the current thread. Called by `ProfileGuard::drop()`.
pub(crate) fn record_call(fn_name: &str, total_ns: u64, self_ns: u64) {
    if !ENABLED.load(Relaxed) { return; }

    let tid = current_thread_num();
    let tname = current_thread_name();
    let mut data = THREAD_DATA.lock();
    let entry = data.entry(tid).or_insert_with(|| ThreadEntry::new(tname));
    *entry.fn_time_ns.entry(fn_name.to_owned()).or_insert(0) += total_ns;
    *entry.fn_call_count.entry(fn_name.to_owned()).or_insert(0) += 1;
    *entry.fn_self_ns.entry(fn_name.to_owned()).or_insert(0) += self_ns;
    entry.active_ns += self_ns;
    entry.last_seen = Instant::now();
}

// ─── output schema ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadReport {
    pub threads: Vec<ThreadBreakdown>,
    /// Cross-thread summary: which functions ran on multiple threads.
    pub cross_thread_fns: Vec<CrossThreadFn>,
    /// Thread with the most active time (potential bottleneck).
    pub busiest_thread: Option<String>,
    /// Load imbalance coefficient: stddev / mean of active times (0 = perfect).
    pub load_imbalance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadBreakdown {
    pub thread_id: u64,
    pub thread_name: String,
    /// Total active (self) time in nanoseconds.
    pub active_ns: u64,
    /// % of session wall time this thread was active.
    pub active_pct: f64,
    /// Per-function breakdown for this thread.
    pub functions: Vec<ThreadFnRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadFnRecord {
    pub name: String,
    pub call_count: u64,
    pub total_ns: u64,
    pub self_ns: u64,
    pub mean_ns: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossThreadFn {
    pub name: String,
    pub thread_count: u32,
    pub total_ns_across_threads: u64,
    /// True if this function ran on >1 thread simultaneously at some point.
    /// (Approximated by checking if thread_count > 1.)
    pub concurrent: bool,
}

// ─── ThreadProfiler API ───────────────────────────────────────────────────────

/// Returns true if ThreadProfiler::enable() has been called.
pub fn is_enabled() -> bool {
    ENABLED.load(Relaxed)
}

pub struct ThreadProfiler;

impl ThreadProfiler {
    pub fn enable() {
        ENABLED.store(true, Relaxed);
    }

    pub fn collect(session_ns: u64) -> ThreadReport {
        let data = THREAD_DATA.lock();
        let mut threads: Vec<ThreadBreakdown> = Vec::new();
        let mut fn_thread_map: HashMap<String, Vec<u64>> = HashMap::new(); // fn → thread active_ns per thread

        for (tid, entry) in data.iter() {
            let mut fns: Vec<ThreadFnRecord> = entry.fn_time_ns.iter().map(|(name, &total)| {
                let calls = entry.fn_call_count.get(name).copied().unwrap_or(1);
                let self_ns = entry.fn_self_ns.get(name).copied().unwrap_or(0);
                fn_thread_map.entry(name.clone()).or_default().push(total);
                ThreadFnRecord {
                    name: name.clone(),
                    call_count: calls,
                    total_ns: total,
                    self_ns,
                    mean_ns: total as f64 / calls as f64,
                }
            }).collect();
            fns.sort_by(|a, b| b.total_ns.cmp(&a.total_ns));

            let active_pct = if session_ns > 0 {
                entry.active_ns as f64 / session_ns as f64 * 100.0
            } else { 0.0 };

            threads.push(ThreadBreakdown {
                thread_id: *tid,
                thread_name: entry.thread_name.clone(),
                active_ns: entry.active_ns,
                active_pct,
                functions: fns,
            });
        }
        threads.sort_by(|a, b| b.active_ns.cmp(&a.active_ns));

        let busiest = threads.first().map(|t| t.thread_name.clone());

        // Load imbalance
        let active_times: Vec<f64> = threads.iter().map(|t| t.active_ns as f64).collect();
        let mean = if !active_times.is_empty() {
            active_times.iter().sum::<f64>() / active_times.len() as f64
        } else { 0.0 };
        let variance = active_times.iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>() / active_times.len().max(1) as f64;
        let imbalance = if mean > 0.0 { variance.sqrt() / mean } else { 0.0 };

        // Cross-thread functions
        let mut cross: Vec<CrossThreadFn> = fn_thread_map.iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(name, v)| CrossThreadFn {
                name: name.clone(),
                thread_count: v.len() as u32,
                total_ns_across_threads: v.iter().sum(),
                concurrent: v.len() > 1,
            })
            .collect();
        cross.sort_by(|a, b| b.total_ns_across_threads.cmp(&a.total_ns_across_threads));

        ThreadReport {
            threads,
            cross_thread_fns: cross,
            busiest_thread: busiest,
            load_imbalance: imbalance,
        }
    }

    pub fn save_json(path: &str) -> std::io::Result<()> {
        let session = crate::global::GLOBAL_PROFILER.collect();
        let report = Self::collect(session.session_duration_ns);
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)?;
        println!("[rustscope] Thread profile saved to: {}", path);
        Ok(())
    }
}
