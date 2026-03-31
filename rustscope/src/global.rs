//! Global profiler state and the `ProfileGuard` RAII type.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::cell::RefCell;

use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::collectors::{
    memory::{self, AllocSnapshot},
    cpu::CpuCounterGuard,
    stack,
    timing::TimingAccumulator,
    locks,
};
use crate::output::{
    schema::{
        CallTreeNode, CpuCounters, FunctionRecord, MemoryMetrics,
        StackMetrics,
    },
    writer::{print_summary, WriteOptions, write_json},
};
use crate::output::schema::{
    HostInfo, ProfileSession, SessionMemory, BenchmarkRecord,
};

// ─── global singleton ────────────────────────────────────────────────────────

pub static GLOBAL_PROFILER: Lazy<GlobalProfiler> = Lazy::new(GlobalProfiler::new);

// ─── per-thread call stack ────────────────────────────────────────────────────

struct Frame {
    name: &'static str,
    file: &'static str,
    line: u32,
    module: &'static str,
    start_ns: Instant,
    /// Absolute offset from session start — for timeline event timestamps.
    start_offset_ns: u64,
    sp_at_entry: Option<usize>,
    alloc_at_entry: AllocSnapshot,
    cpu_guard: Option<CpuCounterGuard>,
    /// Nanoseconds charged to child calls (for self-time computation).
    child_ns: u64,
    /// Recursion depth at entry.
    depth_at_entry: u32,
    /// Sequential call index for this function (for outlier tracking).
    call_index: u64,
    /// Children collected during this frame's execution.
    children: Vec<CallTreeNode>,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = RefCell::new(Vec::with_capacity(64));
    static ROOTS: RefCell<Vec<CallTreeNode>> = RefCell::new(Vec::new());
}

// ─── global profiler ─────────────────────────────────────────────────────────

pub struct GlobalProfiler {
    /// Per-function accumulated stats.
    records: RwLock<HashMap<String, FunctionState>>,
    /// Call-tree root nodes (accumulated from all threads).
    call_trees: RwLock<Vec<CallTreeNode>>,
    /// Benchmark results.
    benchmarks: RwLock<Vec<BenchmarkRecord>>,
    pub(crate) start_time: Instant,
    start_unix: u64,
    enabled: AtomicBool,
}

struct FunctionState {
    record: FunctionRecord,      // identity fields pre-filled
    timing: TimingAccumulator,
    max_recursion: u32,
    max_depth: u32,
    total_alloc: u64,
    total_dealloc: u64,
    total_alloc_ops: u64,
    peak_delta: u64,
    cpu: Option<CpuCounters>,    // accumulated CPU counter totals
}

impl FunctionState {
    fn new(name: &str, file: &str, line: u32, module: &str) -> Self {
        let r = FunctionRecord {
            name: name.to_owned(),
            file: file.to_owned(),
            line,
            module_path: module.to_owned(),
            call_count: 0,
            max_recursion_depth: 1,
            timing: Default::default(),
            memory: None,
            stack: Default::default(),
            cpu: None,
            outlier_count: 0,
        };
        Self {
            record: r,
            timing: TimingAccumulator::new(),
            max_recursion: 1,
            max_depth: 0,
            total_alloc: 0,
            total_dealloc: 0,
            total_alloc_ops: 0,
            peak_delta: 0,
            cpu: None,
        }
    }
}

impl GlobalProfiler {
    fn new() -> Self {
        let start_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            records:    RwLock::new(HashMap::new()),
            call_trees: RwLock::new(Vec::new()),
            benchmarks: RwLock::new(Vec::new()),
            start_time: Instant::now(),
            start_unix,
            enabled: AtomicBool::new(false),
        }
    }

    pub fn enable(&self) { self.enabled.store(true, Relaxed); }
    pub fn is_enabled(&self) -> bool { self.enabled.load(Relaxed) }

    /// Called by `ProfileGuard::exit()` with all collected data.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        name: &str,
        file: &str,
        line: u32,
        module: &str,
        total_ns: u64,
        self_ns: u64,
        frame_size: Option<u64>,
        recursion_depth: u32,
        call_depth: u32,
        alloc_snap_entry: &AllocSnapshot,
        alloc_snap_exit: &AllocSnapshot,
        cpu: Option<CpuCounters>,
    ) {
        let key = format!("{}:{}", module, name);
        let mut records = self.records.write();
        let state = records.entry(key)
            .or_insert_with(|| FunctionState::new(name, file, line, module));

        state.record.call_count += 1;
        state.timing.record(total_ns, self_ns);

        if recursion_depth > state.max_recursion {
            state.max_recursion = recursion_depth;
        }
        if call_depth > state.max_depth {
            state.max_depth = call_depth;
        }

        // Memory
        if memory::ALLOCATOR_ACTIVE.load(Relaxed) {
            let alloc_delta   = alloc_snap_exit.alloc_delta(alloc_snap_entry);
            let dealloc_delta = alloc_snap_exit.dealloc_delta(alloc_snap_entry);
            let ops_delta     = alloc_snap_exit.ops_delta(alloc_snap_entry);
            let peak_d        = alloc_snap_exit.peak_delta(alloc_snap_entry);
            state.total_alloc   += alloc_delta;
            state.total_dealloc += dealloc_delta;
            state.total_alloc_ops += ops_delta;
            if peak_d > state.peak_delta { state.peak_delta = peak_d; }
        }

        // Stack
        if let Some(fs) = frame_size {
            if state.record.stack.frame_size_bytes.map_or(true, |e| fs > e) {
                state.record.stack.frame_size_bytes = Some(fs);
            }
        }
        state.record.stack.max_call_depth = state.max_depth;

        // CPU — accumulate totals
        if let Some(c) = cpu {
            let acc = state.cpu.get_or_insert_with(Default::default);
            acc.cpu_cycles           += c.cpu_cycles;
            acc.instructions         += c.instructions;
            acc.cache_references     += c.cache_references;
            acc.cache_misses         += c.cache_misses;
            acc.l1_dcache_loads      += c.l1_dcache_loads;
            acc.l1_dcache_load_misses+= c.l1_dcache_load_misses;
            acc.llc_loads            += c.llc_loads;
            acc.llc_load_misses      += c.llc_load_misses;
            acc.branch_instructions  += c.branch_instructions;
            acc.branch_misses        += c.branch_misses;
            acc.context_switches     += c.context_switches;
            acc.page_faults          += c.page_faults;
            acc.cpu_migrations       += c.cpu_migrations;
        }
    }

    pub fn push_call_tree_root(&self, node: CallTreeNode) {
        self.call_trees.write().push(node);
    }

    pub fn push_benchmark(&self, rec: BenchmarkRecord) {
        self.benchmarks.write().push(rec);
    }

    /// Assemble the complete `ProfileSession`.
    pub fn collect(&self) -> ProfileSession {
        let session_ns = self.start_time.elapsed().as_nanos() as u64;
        let records = self.records.read();

        // Build outlier count map once (avoids O(n²) re-fetch per function)
        let outlier_counts: HashMap<String, u64> = {
            let recs = crate::features::outliers::get_outliers();
            let mut m: HashMap<String, u64> = HashMap::new();
            for o in recs { *m.entry(o.function).or_insert(0) += 1; }
            m
        };

        let functions: Vec<FunctionRecord> = records.values().map(|state| {
            let mut rec = state.record.clone();
            rec.timing = state.timing.build_stats(session_ns);
            rec.max_recursion_depth = state.max_recursion;
            rec.outlier_count = *outlier_counts.get(&rec.name).unwrap_or(&0);

            if memory::ALLOCATOR_ACTIVE.load(Relaxed) && state.total_alloc_ops > 0 {
                rec.memory = Some(MemoryMetrics {
                    total_alloc_bytes:   state.total_alloc,
                    total_dealloc_bytes: state.total_dealloc,
                    net_retained_bytes:  state.total_alloc as i64 - state.total_dealloc as i64,
                    peak_delta_bytes:    state.peak_delta as i64,
                    alloc_count: state.total_alloc_ops,
                    dealloc_count: 0, // Not tracked separately
                    mean_alloc_per_call: if state.record.call_count > 0 {
                        state.total_alloc as f64 / state.record.call_count as f64
                    } else { 0.0 },
                    alloc_op_count: state.total_alloc_ops,
                });
            }

            if let Some(c) = &state.cpu {
                let mut avg = c.clone();
                avg.ipc = if avg.cpu_cycles > 0 {
                    avg.instructions as f64 / avg.cpu_cycles as f64
                } else { 0.0 };
                avg.cache_miss_rate = if avg.cache_references > 0 {
                    avg.cache_misses as f64 / avg.cache_references as f64
                } else { 0.0 };
                avg.branch_miss_rate = if avg.branch_instructions > 0 {
                    avg.branch_misses as f64 / avg.branch_instructions as f64
                } else { 0.0 };
                rec.cpu = Some(avg);
            }

            rec
        }).collect();

        let alloc_now = memory::snapshot();
        let session_memory = SessionMemory {
            peak_rss_mb:         alloc_now.peak as f64 / (1024.0 * 1024.0),
            peak_heap_bytes:     alloc_now.peak,
            final_heap_bytes:    alloc_now.current.max(0) as u64,
            total_alloc_bytes:   alloc_now.total_alloc,
            total_dealloc_bytes: alloc_now.total_dealloc,
            total_alloc_ops:     alloc_now.alloc_ops,
        };

        // Collect async spans if the feature + layer are active
        #[cfg(feature = "async-profiling")]
        let async_spans = {
            let recs = crate::async_profile::collect_async_records();
            if recs.is_empty() { None }
            else { serde_json::to_value(recs).ok() }
        };
        #[cfg(not(feature = "async-profiling"))]
        let async_spans: Option<serde_json::Value> = None;

        // Collect thread breakdown if ThreadProfiler was enabled
        let thread_report = if crate::thread_profile::is_enabled() {
            let tr = crate::thread_profile::ThreadProfiler::collect(session_ns);
            serde_json::to_value(tr).ok()
        } else {
            None
        };

        // Session metadata
        let session_meta = if !crate::features::metadata::is_empty() {
            serde_json::to_value(crate::features::metadata::get()).ok()
        } else {
            None
        };

        // Outlier + budget summary
        let outlier_sum = crate::features::outliers::build_summary();
        let outlier_summary = if outlier_sum.total_outliers > 0 || outlier_sum.total_budget_violations > 0 {
            serde_json::to_value(&outlier_sum).ok()
        } else {
            None
        };

        // Timeline summary
        let timeline_summary = if crate::features::timeline::is_enabled() {
            serde_json::to_value(crate::features::timeline::summarize()).ok()
        } else {
            None
        };

        let async_tasks = {
            #[cfg(feature = "async-profiling")]
            {
                crate::async_profile::collect_async_tasks()
            }
            #[cfg(not(feature = "async-profiling"))]
            {
                Vec::new()
            }
        };

        ProfileSession {
            schema_version: 3,
            started_at_unix_secs: self.start_unix,
            session_duration_ns: session_ns,
            host: build_host_info(),
            functions,
            benchmarks: self.benchmarks.read().clone(),
            call_trees: self.call_trees.read().clone(),
            session_memory,
            async_spans,
            thread_report,
            session_meta,
            outlier_summary,
            timeline_summary,
            locks: locks::snapshot(),
            async_tasks,
            process_summary: None,
            process_samples: Vec::new(),
            memory_events: Vec::new(),
            crate_rollups: Vec::new(),
            module_rollups: Vec::new(),
        }
    }
}

fn build_host_info() -> HostInfo {
    HostInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_logical_cores: num_cpus(),
        rustc_version: env!("RUSTC_VERSION_FOR_RUSTSCOPE").to_string(),
        build_profile: if cfg!(debug_assertions) { "debug".into() } else { "release".into() },
    }
}

fn num_cpus() -> u32 {
    // Poor man's core count: read /proc/cpuinfo on Linux, fallback to 1.
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count() as u32
    }
    #[cfg(not(target_os = "linux"))]
    { 1 }
}

// ─── public API ──────────────────────────────────────────────────────────────

/// Top-level API.
pub struct Profiler;

impl Profiler {
    /// Must be called before any profiled code runs.
    pub fn init() {
        GLOBAL_PROFILER.enable();
    }

    /// Convenience helper: initialize the profiler, run `f`, then write a single
    /// JSON file on disk with the collected session.
    ///
    /// This is the easiest way to "just run some code and get one JSON report".
    /// Typical usage in a binary:
    ///
    /// ```rust,no_run
    /// fn main() {
    ///     rustscope::Profiler::run_with_json("profile.json", || {
    ///         // Your application / benchmark code here.
    ///         run_app();
    ///     }).expect("failed to write profile.json");
    /// }
    /// ```
    pub fn run_with_json<F, T>(path: &str, f: F) -> std::io::Result<T>
    where
        F: FnOnce() -> T,
    {
        Self::init();
        let result = f();
        Self::save_json(path)?;
        Ok(result)
    }

    /// Run a long-lived workload until the user presses Ctrl+C, then
    /// write a single JSON report to `path`.
    ///
    /// This lets the user decide how long to run the program in the
    /// terminal. When they interrupt with Ctrl+C, RustScope captures
    /// the final session and writes the report before exiting.
    pub fn run_until_ctrl_c<F>(path: &str, mut tick: F) -> std::io::Result<()>
    where
        F: FnMut(),
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        Self::init();

        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();

        ctrlc::set_handler(move || {
            flag.store(false, Ordering::SeqCst);
        }).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        while running.load(Ordering::SeqCst) {
            tick();
        }

        Self::save_json(path)
    }

    /// Write the full session as pretty-printed JSON.
    pub fn save_json(path: &str) -> std::io::Result<()> {
        let session = GLOBAL_PROFILER.collect();
        write_json(std::path::Path::new(path), &session, &WriteOptions::default())
    }

    /// Write compact JSON (smaller file, no indentation).
    pub fn save_json_compact(path: &str) -> std::io::Result<()> {
        let session = GLOBAL_PROFILER.collect();
        write_json(std::path::Path::new(path), &session, &WriteOptions {
            pretty: false, ..Default::default()
        })
    }

    /// Append one NDJSON record (for long-running or repeated sessions).
    pub fn append_json(path: &str) -> std::io::Result<()> {
        let session = GLOBAL_PROFILER.collect();
        write_json(std::path::Path::new(path), &session, &WriteOptions {
            append: true, pretty: false, ..Default::default()
        })
    }

    /// Print a human-readable summary table to stdout.
    pub fn print_summary() {
        print_summary(&GLOBAL_PROFILER.collect());
    }

    /// Return the raw session data for custom processing.
    pub fn collect() -> ProfileSession {
        GLOBAL_PROFILER.collect()
    }

    /// Reset all accumulated profiling data (keeps profiler enabled).
    /// Useful between phases of a long-running process.
    pub fn reset() {
        GLOBAL_PROFILER.records.write().clear();
        GLOBAL_PROFILER.call_trees.write().clear();
        GLOBAL_PROFILER.benchmarks.write().clear();
        crate::features::timeline::reset();
        crate::features::outliers::reset();
    }

    /// Attach a string tag to this session (appears in JSON under session_meta).
    pub fn tag(label: &str) {
        crate::features::metadata::tag(label);
    }

    /// Attach a key-value pair to this session.
    pub fn meta(key: &str, value: &str) {
        crate::features::metadata::set(key, value);
    }

    /// Register a per-call latency budget. Violations are flagged in the timeline
    /// and collected in the outlier report.
    pub fn set_budget(fn_name: &str, budget_ns: u64) {
        crate::features::outliers::set_budget(fn_name, budget_ns);
    }

    /// Auto-detect CI environment and capture git metadata.
    pub fn detect_ci() {
        crate::features::metadata::auto_detect_ci();
    }

    /// Export the current session as a gzip pprof file (`go tool pprof` compatible).
    pub fn save_pprof(path: &str) -> std::io::Result<()> {
        let session = GLOBAL_PROFILER.collect();
        crate::features::pprof_export::save(&session, path)
    }

    /// Save the full timeline as NDJSON (one event per line).
    pub fn save_timeline(path: &str) -> std::io::Result<()> {
        crate::features::timeline::save(path)
    }

    /// Save only slow calls (>= min_ns) to a timeline NDJSON file.
    pub fn save_slow_timeline(path: &str, min_ns: u64) -> std::io::Result<()> {
        crate::features::timeline::save_slow(path, min_ns)
    }
}

// ─── ProfileGuard ─────────────────────────────────────────────────────────────

/// RAII guard. Created at function entry, drives all metric collection.
/// Do NOT store this in a variable named with a leading `_` — Rust drops
/// `_foo` immediately. Use `let _guard = ...;` (with a name).
pub struct ProfileGuard {
    name: &'static str,
    file: &'static str,
    line: u32,
    module: &'static str,
}

impl ProfileGuard {
    /// Called by `#[profile]` and `profile_scope!()`.
    #[inline]
    pub fn enter(
        name: &'static str,
        file: &'static str,
        line: u32,
        module: &'static str,
    ) -> Self {
        if GLOBAL_PROFILER.is_enabled() {
            let sp = stack::read_sp();
            let depth = stack::push_depth();
            let alloc = memory::snapshot();
            let cpu = CpuCounterGuard::open();

            let now = Instant::now();
            let offset = GLOBAL_PROFILER.start_time.elapsed().as_nanos() as u64;
            STACK.with(|s| s.borrow_mut().push(Frame {
                name,
                file,
                line,
                module,
                start_ns: now,
                start_offset_ns: offset,
                sp_at_entry: sp,
                alloc_at_entry: alloc,
                cpu_guard: Some(cpu),
                child_ns: 0,
                depth_at_entry: depth,
                call_index: 0, // filled in on drop from FunctionState.record.call_count
                children: Vec::new(),
            }));
        }
        Self { name, file, line, module }
    }

    /// Called when the guard is dropped (manually or via end of scope).
    #[inline]
    pub fn exit(self) {
        // Drop impl does the work.
    }
}

impl Drop for ProfileGuard {
    #[inline]
    fn drop(&mut self) {
        if !GLOBAL_PROFILER.is_enabled() { return; }

        let sp_now = stack::read_sp();

        STACK.with(|stack_cell| {
            let mut stack = stack_cell.borrow_mut();
            let frame = match stack.pop() {
                Some(f) => f,
                None => return,
            };

            let total_ns = frame.start_ns.elapsed().as_nanos() as u64;
            let self_ns  = total_ns.saturating_sub(frame.child_ns);

            // Charge our total to parent's child_ns counter
            if let Some(parent) = stack.last_mut() {
                parent.child_ns += total_ns;
            }

            let frame_size_bytes = frame.sp_at_entry
                .zip(sp_now)
                .map(|(entry, now)| stack::frame_size(entry, now));

            let alloc_exit = memory::snapshot();
            let cpu = frame.cpu_guard.and_then(|g| g.read());

            let recursion_depth = frame.depth_at_entry;
            let call_depth = stack.len() as u32;
            stack::pop_depth();

            // Build a call tree node
            let node = CallTreeNode {
                name: frame.name.to_owned(),
                call_count: 1,
                total_ns,
                file: frame.file.to_owned(),
                line: frame.line,
                duration_ns: total_ns,
                alloc_bytes: if memory::ALLOCATOR_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                    Some(alloc_exit.alloc_delta(&frame.alloc_at_entry))
                } else {
                    None
                },
                cpu_cycles: cpu.as_ref().map(|c| c.cpu_cycles),
                children: frame.children,
            };

            if let Some(parent) = stack.last_mut() {
                parent.children.push(node);
            } else {
                // We're a root call — push to global tree
                GLOBAL_PROFILER.push_call_tree_root(node);
            }

            GLOBAL_PROFILER.record(
                frame.name,
                frame.file,
                frame.line,
                frame.module,
                total_ns,
                self_ns,
                frame_size_bytes,
                recursion_depth,
                call_depth,
                &frame.alloc_at_entry,
                &alloc_exit,
                cpu,
            );

            // Phase 4: feed thread profiler (zero-cost when disabled)
            crate::thread_profile::record_call(frame.name, total_ns, self_ns);

            // Features: timeline + outlier detection (zero-cost when disabled)
            let alloc_bytes  = if memory::ALLOCATOR_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                alloc_exit.alloc_delta(&frame.alloc_at_entry)
            } else { 0 };
            let dealloc_bytes = if memory::ALLOCATOR_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                alloc_exit.dealloc_delta(&frame.alloc_at_entry)
            } else { 0 };
            let thread_num = crate::thread_profile::current_thread_num();

            // Get call index from the record we just updated
            let call_idx = {
                let records = GLOBAL_PROFILER.records.read();
                let key = format!("{}:{}", frame.module, frame.name);
                records.get(&key).map(|s| s.record.call_count).unwrap_or(0)
            };

            let (is_outlier, budget_exceeded) = crate::features::outliers::check(
                frame.name,
                total_ns,
                call_idx,
                thread_num,
                frame.start_offset_ns,
            );

            crate::features::timeline::record(
                frame.name,
                frame.start_offset_ns,
                total_ns,
                self_ns,
                thread_num,
                call_depth,
                alloc_bytes,
                dealloc_bytes,
                is_outlier,
                budget_exceeded,
            );
        });
    }
}
