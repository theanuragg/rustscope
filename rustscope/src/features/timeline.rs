//! # Timeline: ordered per-call event log
//!
//! Unlike the aggregate stats in `functions[]`, the timeline records
//! **every individual function call** with its exact start timestamp,
//! duration, thread, and heap delta.
//!
//! ## When to use
//! - Debug a latency spike on the 1000th call (aggregates miss this)
//! - Understand ordering between concurrent operations
//! - Correlate a slow call with heap growth at a specific moment
//! - Find which specific invocation exceeded your SLO
//!
//! ## Output format
//! Written as NDJSON (one JSON object per line) to keep it streamable
//! and avoid building a huge Vec in memory.
//!
//! ```json
//! {"t":0,"name":"parse","dur_ns":4200,"self_ns":4200,"thread":1,"depth":0,"alloc_bytes":0}
//! {"t":4200,"name":"validate","dur_ns":812,"self_ns":812,"thread":1,"depth":0,"alloc_bytes":128}
//! ```
//!
//! ## Usage
//! ```rust
//! rustscope::Profiler::init();
//! rustscope::timeline::enable();          // opt-in (has memory cost)
//!
//! // ... run code ...
//!
//! rustscope::timeline::save("timeline.ndjson").unwrap();
//! // or: rustscope::timeline::save_filtered("timeline.ndjson", |e| e.dur_ns > 10_000)
//! ```

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

fn is_false(b: &bool) -> bool { !*b }

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// ─── config ───────────────────────────────────────────────────────────────────

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Maximum events to store in-memory (default 1M). When full, oldest are dropped.
static MAX_EVENTS: AtomicU64 = AtomicU64::new(1_000_000);

pub fn enable() { ENABLED.store(true, Relaxed); }
pub fn disable() { ENABLED.store(false, Relaxed); }
pub fn is_enabled() -> bool { ENABLED.load(Relaxed) }
pub fn set_max_events(n: u64) { MAX_EVENTS.store(n, Relaxed); }

// ─── event ────────────────────────────────────────────────────────────────────

/// One call event. Designed to be small (64 bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Nanoseconds since `Profiler::init()` — the call's start time.
    pub t: u64,
    /// Function name.
    pub name: String,
    /// Total inclusive duration (ns).
    pub dur_ns: u64,
    /// Self time (ns) — excludes time spent in callees.
    pub self_ns: u64,
    /// Thread number (1-based, sequential per process).
    pub thread: u64,
    /// Call stack depth at entry (0 = root call).
    pub depth: u32,
    /// Heap bytes allocated during this call (0 if TrackingAllocator not installed).
    pub alloc_bytes: u64,
    /// Heap bytes freed during this call.
    pub dealloc_bytes: u64,
    /// True if this call was flagged as an outlier (> 3σ above the mean for this fn).
    #[serde(skip_serializing_if = "is_false")]
    pub outlier: bool,
    /// True if this call exceeded its configured budget_ns.
    #[serde(skip_serializing_if = "is_false")]
    pub budget_exceeded: bool,
}

// ─── global ring buffer ───────────────────────────────────────────────────────

static EVENTS: Lazy<Mutex<Vec<TimelineEvent>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SESSION_START: Lazy<Instant> = Lazy::new(Instant::now);

/// Called by ProfileGuard::drop() — zero-cost when timeline is disabled.
#[inline]
pub(crate) fn record(
    name: &str,
    t_offset_ns: u64,
    dur_ns: u64,
    self_ns: u64,
    thread: u64,
    depth: u32,
    alloc_bytes: u64,
    dealloc_bytes: u64,
    outlier: bool,
    budget_exceeded: bool,
) {
    if !ENABLED.load(Relaxed) { return; }

    let ev = TimelineEvent {
        t: t_offset_ns,
        name: name.to_owned(),
        dur_ns,
        self_ns,
        thread,
        depth,
        alloc_bytes,
        dealloc_bytes,
        outlier,
        budget_exceeded,
    };

    let mut events = EVENTS.lock();
    let max = MAX_EVENTS.load(Relaxed) as usize;
    if events.len() >= max {
        // Drop oldest 10% to avoid one big allocation per event at the limit
        let drain_to = max / 10;
        events.drain(0..drain_to);
    }
    events.push(ev);
}

/// Get the nanoseconds elapsed since the profiler session started.
#[inline]
pub(crate) fn session_offset_ns() -> u64 {
    SESSION_START.elapsed().as_nanos() as u64
}

/// Clear all recorded events.
pub fn reset() {
    EVENTS.lock().clear();
}

/// Write all events as NDJSON to `path`.
pub fn save(path: &str) -> std::io::Result<()> {
    save_filtered(path, |_| true)
}

/// Write events matching `predicate` as NDJSON to `path`.
pub fn save_filtered<F>(path: &str, predicate: F) -> std::io::Result<()>
where
    F: Fn(&TimelineEvent) -> bool,
{
    let events = EVENTS.lock();
    let mut file = std::fs::File::create(path)?;
    let mut count = 0usize;
    for ev in events.iter() {
        if predicate(ev) {
            let line = serde_json::to_string(ev)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
            count += 1;
        }
    }
    println!("[rustscope/timeline] {} events written to {}", count, path);
    Ok(())
}

/// Write only events where `dur_ns >= min_ns` (fast hot-path filter).
pub fn save_slow(path: &str, min_ns: u64) -> std::io::Result<()> {
    save_filtered(path, |e| e.dur_ns >= min_ns)
}

/// Write only outlier events.
pub fn save_outliers(path: &str) -> std::io::Result<()> {
    save_filtered(path, |e| e.outlier)
}

/// Write only budget-exceeded events.
pub fn save_budget_exceeded(path: &str) -> std::io::Result<()> {
    save_filtered(path, |e| e.budget_exceeded)
}

/// Returns a snapshot of all events (clones — for small datasets only).
pub fn collect() -> Vec<TimelineEvent> {
    EVENTS.lock().clone()
}

/// Count of recorded events currently in memory.
pub fn event_count() -> usize {
    EVENTS.lock().len()
}

/// Summary statistics over all timeline events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSummary {
    pub total_events: usize,
    pub outlier_count: usize,
    pub budget_exceeded_count: usize,
    pub unique_functions: usize,
    pub slowest_call: Option<TimelineEvent>,
    pub most_allocating_call: Option<TimelineEvent>,
    pub total_alloc_bytes: u64,
    pub timeline_span_ns: u64,
}

pub fn summarize() -> TimelineSummary {
    let events = EVENTS.lock();
    let mut outlier_count = 0usize;
    let mut budget_exceeded_count = 0usize;
    let mut unique_fns = std::collections::HashSet::new();
    let mut slowest: Option<&TimelineEvent> = None;
    let mut most_alloc: Option<&TimelineEvent> = None;
    let mut total_alloc = 0u64;
    let mut span_end = 0u64;

    for ev in events.iter() {
        if ev.outlier { outlier_count += 1; }
        if ev.budget_exceeded { budget_exceeded_count += 1; }
        unique_fns.insert(ev.name.as_str());
        total_alloc += ev.alloc_bytes;
        let end = ev.t + ev.dur_ns;
        if end > span_end { span_end = end; }
        if slowest.map_or(true, |s: &TimelineEvent| ev.dur_ns > s.dur_ns) {
            slowest = Some(ev);
        }
        if most_alloc.map_or(true, |s: &TimelineEvent| ev.alloc_bytes > s.alloc_bytes) {
            most_alloc = Some(ev);
        }
    }

    TimelineSummary {
        total_events: events.len(),
        outlier_count,
        budget_exceeded_count,
        unique_functions: unique_fns.len(),
        slowest_call: slowest.cloned(),
        most_allocating_call: most_alloc.cloned(),
        total_alloc_bytes: total_alloc,
        timeline_span_ns: span_end,
    }
}
