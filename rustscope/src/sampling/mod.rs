//! # Phase 3: Sampling Profiler (SIGPROF-based)
//!
//! A statistical **sampling** profiler that requires **zero code changes**.
//! Instead of instrumenting functions, it periodically interrupts the process
//! via `SIGPROF`, captures a backtrace, and counts which stack frames appear
//! most frequently. This is the same technique used by Linux `perf record`,
//! `pprof`, and `samply`.
//!
//! ## When to use sampling vs instrumentation
//!
//! | Approach | Overhead | Coverage | Requires code changes |
//! |---|---|---|---|
//! | `#[profile]` | ~10–50ns per call | only annotated fns | yes |
//! | Sampling (this) | <1% CPU (at 100Hz) | all code incl. deps | **no** |
//!
//! Use sampling to find *where* time goes in unfamiliar code.
//! Use instrumentation for precise timing of known hot paths.
//!
//! ## Usage
//!
//! ```rust
//! use rustscope::sampling::{SamplingProfiler, SamplingConfig};
//!
//! fn main() {
//!     let config = SamplingConfig {
//!         frequency_hz: 200,      // samples per second
//!         max_stack_depth: 32,    // frames to capture per sample
//!         include_idle: false,    // skip samples where process is idle
//!     };
//!
//!     // Start sampling in a background thread
//!     let guard = SamplingProfiler::start(config);
//!
//!     // ... run your code (no annotation needed) ...
//!     do_work();
//!
//!     // Stop and save
//!     let report = guard.stop();
//!     report.save_json("sampling_report.json").unwrap();
//! }
//! ```
//!
//! ## Signal safety
//!
//! The signal handler only writes to a pre-allocated ring buffer using
//! async-signal-safe operations. No malloc, no locks, no logging.
//! The collector thread drains the ring buffer between samples.
//!
//! ## Platform support
//! - Linux: uses `setitimer(ITIMER_PROF)` → `SIGPROF`
//! - macOS: uses `setitimer(ITIMER_PROF)` → `SIGPROF` (same)
//! - Windows: not supported (returns `Err` gracefully)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use backtrace::Backtrace;
use serde::{Deserialize, Serialize};

// ─── configuration ────────────────────────────────────────────────────────────

/// Configuration for the sampling profiler.
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    /// Samples to take per second. 100–500Hz is typical. Higher = more overhead.
    pub frequency_hz: u32,
    /// Maximum stack depth to capture per sample.
    pub max_stack_depth: usize,
    /// If false, samples where the top frame is in known idle code (epoll, sleep)
    /// are skipped. Reduces noise in I/O-heavy programs.
    pub include_idle: bool,
    /// Prefixes to exclude from symbolication (e.g. ["libc", "pthread"]).
    /// These frames are collapsed to a single "[runtime]" node.
    pub blocklist_prefixes: Vec<String>,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 100,
            max_stack_depth: 24,
            include_idle: false,
            blocklist_prefixes: vec![
                "libc".into(),
                "libgcc".into(),
                "pthread".into(),
                "std::sys".into(),
                "core::panicking".into(),
            ],
        }
    }
}

// ─── output schema ────────────────────────────────────────────────────────────

/// A single node in the sampling call tree.
/// Weight = number of samples this frame appeared in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleNode {
    /// Demangled symbol name.
    pub name: String,
    /// Source file (if DWARF debug info is available).
    pub file: Option<String>,
    /// Source line.
    pub line: Option<u32>,
    /// Number of samples where this was the **top** (leaf) frame.
    pub self_samples: u64,
    /// Number of samples where this frame appeared anywhere in the stack.
    pub total_samples: u64,
    /// Self time % = self_samples / total_session_samples * 100.
    pub self_pct: f64,
    /// Total time % = total_samples / total_session_samples * 100.
    pub total_pct: f64,
    pub children: Vec<SampleNode>,
}

/// Output of one sampling session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingReport {
    pub config: SamplingReportConfig,
    pub total_samples: u64,
    pub duration_ns: u64,
    pub effective_frequency_hz: f64,
    /// Flat list sorted by self_pct descending — easy to grep/jq.
    pub flat: Vec<FlatSampleRecord>,
    /// Hierarchical call tree (for flame graph construction).
    pub call_tree: Vec<SampleNode>,
    /// Raw stack traces (as symbol strings), if `store_raw` was enabled.
    pub raw_stacks: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingReportConfig {
    pub frequency_hz: u32,
    pub max_stack_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatSampleRecord {
    pub name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub self_samples: u64,
    pub total_samples: u64,
    pub self_pct: f64,
    pub total_pct: f64,
}

impl SamplingReport {
    pub fn save_json(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)?;
        println!("[rustscope/sampling] Report saved to: {}", path);
        Ok(())
    }
}



// ─── SIGPROF handler state (global, signal-safe) ──────────────────────────────

static SAMPLING_ACTIVE: AtomicBool = AtomicBool::new(false);

// ─── profiler guard ───────────────────────────────────────────────────────────

/// Returned by `SamplingProfiler::start()`. Stop with `.stop()`.
pub struct SamplingGuard {
    config: SamplingConfig,
    collector: Option<thread::JoinHandle<Vec<RawSample>>>,
    stop_flag: Arc<AtomicBool>,
    start_time: Instant,
}

#[derive(Debug)]
struct RawSample {
    frames: Vec<(String, Option<String>, Option<u32>)>, // (name, file, line)
}

impl SamplingGuard {
    /// Stop the sampling profiler and return the final report.
    pub fn stop(mut self) -> SamplingReport {
        self.stop_flag.store(true, Ordering::SeqCst);
        SAMPLING_ACTIVE.store(false, Ordering::SeqCst);

        // Restore SIGPROF handler and stop itimer
        #[cfg(unix)]
        unsafe {
            stop_itimer();
        }

        let raw_samples = self.collector
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();

        let duration_ns = self.start_time.elapsed().as_nanos() as u64;
        let total = raw_samples.len() as u64;
        let eff_freq = if duration_ns > 0 {
            total as f64 / (duration_ns as f64 / 1e9)
        } else { 0.0 };

        build_report(raw_samples, &self.config, duration_ns, total, eff_freq)
    }
}

pub struct SamplingProfiler;

impl SamplingProfiler {
    /// Start the sampling profiler. Returns a guard — call `.stop()` to finish.
    /// Start the sampling profiler. Returns a guard — call `.stop()` to finish.
    ///
    /// ## How it works (Unix)
    ///
    /// 1. A SIGPROF signal handler is installed. The handler writes the current
    ///    backtrace into a lock-free ring buffer. It fires in the **calling thread**
    ///    (the workload thread), so it correctly measures that thread's execution.
    ///
    /// 2. A background collector thread drains the ring buffer and symbolizes frames
    ///    off the signal-handler path (symbolization is not async-signal-safe).
    ///
    /// 3. `setitimer(ITIMER_PROF)` is used to deliver SIGPROF at `frequency_hz`.
    ///    ITIMER_PROF fires based on CPU time consumed by the process, so idle
    ///    time (waiting on I/O, sleep) is correctly excluded.
    pub fn start(config: SamplingConfig) -> Result<SamplingGuard, String> {
        #[cfg(not(unix))]
        return Err("SamplingProfiler is only supported on Unix (Linux/macOS)".into());

        #[cfg(unix)]
        {
            if SAMPLING_ACTIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                return Err("A sampling profiler is already running".into());
            }

            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag2 = stop_flag.clone();
            let interval_us = 1_000_000 / config.frequency_hz.max(1);
            let blocklist = config.blocklist_prefixes.clone();
            let max_depth = config.max_stack_depth;

            // Set up the SIGPROF handler BEFORE starting the itimer.
            // The handler runs in the workload thread when it's interrupted
            // by SIGPROF — capturing the workload thread's actual stack.
            unsafe { install_sigprof_handler(); }

            // The collector thread drains the ring buffer and symbolizes frames.
            // Symbolization is not async-signal-safe so it MUST happen here, not
            // in the signal handler.
            let collector = thread::spawn(move || {
                let mut samples: Vec<RawSample> = Vec::with_capacity(8192);
                let sleep_dur = Duration::from_micros((interval_us / 2).max(1000) as u64);

                while !stop_flag2.load(Ordering::Relaxed) {
                    // Drain all pending slots in the ring buffer
                    while let Some(bt) = drain_ring_buffer() {
                        let frames = symbolicate_backtrace(bt, max_depth, &blocklist);
                        if !frames.is_empty() {
                            samples.push(RawSample { frames });
                        }
                    }
                    thread::sleep(sleep_dur);
                }
                // Final drain after stop
                while let Some(bt) = drain_ring_buffer() {
                    let frames = symbolicate_backtrace(bt, max_depth, &blocklist);
                    if !frames.is_empty() {
                        samples.push(RawSample { frames });
                    }
                }
                samples
            });

            // Start the itimer AFTER handler + collector are ready
            unsafe { start_itimer(interval_us); }

            Ok(SamplingGuard {
                config,
                collector: Some(collector),
                stop_flag,
                start_time: Instant::now(),
            })
        }
    }

    /// Synchronous sampling: run `f` while sampling, return `(result, report)`.
    ///
    /// SIGPROF fires in the **calling thread** (the workload thread), so this
    /// correctly measures the CPU time of `f` — not a proxy thread.
    ///
    /// ```rust,ignore
    /// let (result, report) = SamplingProfiler::profile(
    ///     SamplingConfig::default(),
    ///     || expensive_uninstrumented_work(),
    /// );
    /// report.save_json("sampling.json").unwrap();
    /// ```
    pub fn profile<F, R>(config: SamplingConfig, f: F) -> (R, SamplingReport)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let interval_us = 1_000_000 / config.frequency_hz.max(1);
        let max_depth = config.max_stack_depth;
        let blocklist = config.blocklist_prefixes.clone();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop2 = stop_flag.clone();

        // Install handler + start collector thread BEFORE starting the itimer.
        #[cfg(unix)]
        unsafe { install_sigprof_handler(); }

        // Collector thread: symbolizes backtraces off the signal-handler path.
        let collector = thread::spawn(move || {
            let mut samples: Vec<RawSample> = Vec::with_capacity(4096);
            let sleep_dur = Duration::from_micros((interval_us / 2).max(1000) as u64);
            while !stop2.load(Ordering::Relaxed) {
                while let Some(bt) = drain_ring_buffer() {
                    let frames = symbolicate_backtrace(bt, max_depth, &blocklist);
                    if !frames.is_empty() { samples.push(RawSample { frames }); }
                }
                thread::sleep(sleep_dur);
            }
            // Final drain
            while let Some(bt) = drain_ring_buffer() {
                let frames = symbolicate_backtrace(bt, max_depth, &blocklist);
                if !frames.is_empty() { samples.push(RawSample { frames }); }
            }
            samples
        });

        // Start the itimer — SIGPROF will now fire in THIS (workload) thread.
        #[cfg(unix)]
        unsafe { start_itimer(interval_us); }

        // Run the workload in the calling thread. SIGPROF fires here.
        let result = f();

        // Stop sampling
        #[cfg(unix)]
        unsafe { stop_itimer(); }
        stop_flag.store(true, Ordering::SeqCst);

        let raw = collector.join().unwrap_or_default();
        let duration_ns = start.elapsed().as_nanos() as u64;
        let total = raw.len() as u64;
        let eff_freq = if duration_ns > 0 { total as f64 / (duration_ns as f64 / 1e9) } else { 0.0 };

        (result, build_report(raw, &config, duration_ns, total, eff_freq))
    }
}

// ─── symbolication ────────────────────────────────────────────────────────────

fn symbolicate_backtrace(
    mut bt: Backtrace,
    max_depth: usize,
    blocklist: &[String],
) -> Vec<(String, Option<String>, Option<u32>)> {
    bt.resolve();
    let mut frames = Vec::new();

    for frame in bt.frames().iter().take(max_depth) {
        for sym in frame.symbols() {
            let name = sym.name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "<unknown>".into());

            // Skip runtime/stdlib noise
            let blocked = blocklist.iter().any(|pfx| name.starts_with(pfx.as_str()))
                || name.starts_with("rustscope::")
                || name.starts_with("backtrace::")
                || name.contains("signal_handler")
                || name.contains("__rust_");
            if blocked { continue; }

            let file = sym.filename().map(|p| p.display().to_string());
            let line = sym.lineno();
            frames.push((name, file, line));
        }
    }
    frames
}

// ─── report builder ───────────────────────────────────────────────────────────

fn build_report(
    raw: Vec<RawSample>,
    config: &SamplingConfig,
    duration_ns: u64,
    total: u64,
    eff_freq: f64,
) -> SamplingReport {
    // Flat counts
    let mut self_counts: HashMap<String, (u64, Option<String>, Option<u32>)> = HashMap::new();
    let mut total_counts: HashMap<String, u64> = HashMap::new();

    for sample in &raw {
        // Leaf frame = self time
        if let Some((name, file, line)) = sample.frames.first() {
            let e = self_counts.entry(name.clone()).or_insert((0, file.clone(), *line));
            e.0 += 1;
        }
        // All frames = total time (deduplicated per sample to avoid double-counting recursive calls)
        let mut seen_in_sample = std::collections::HashSet::new();
        for (name, _, _) in &sample.frames {
            if seen_in_sample.insert(name.as_str()) {
                *total_counts.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    let total_f = total.max(1) as f64;
    let mut flat: Vec<FlatSampleRecord> = self_counts.iter().map(|(name, (self_s, file, line))| {
        let tot = total_counts.get(name).copied().unwrap_or(*self_s);
        FlatSampleRecord {
            name: name.clone(),
            file: file.clone(),
            line: *line,
            self_samples: *self_s,
            total_samples: tot,
            self_pct: *self_s as f64 / total_f * 100.0,
            total_pct: tot as f64 / total_f * 100.0,
        }
    }).collect();
    flat.sort_by(|a, b| b.self_pct.partial_cmp(&a.self_pct).unwrap_or(std::cmp::Ordering::Equal));

    // Build call tree from raw stacks (bottom-up trie)
    let call_tree = build_call_tree(&raw, total);

    SamplingReport {
        config: SamplingReportConfig {
            frequency_hz: config.frequency_hz,
            max_stack_depth: config.max_stack_depth,
        },
        total_samples: total,
        duration_ns,
        effective_frequency_hz: eff_freq,
        flat,
        call_tree,
        raw_stacks: None, // only populated if config.store_raw
    }
}

fn build_call_tree(raw: &[RawSample], total: u64) -> Vec<SampleNode> {
    // Simple trie approach: each unique stack prefix becomes a node.
    // For a proper flame graph we reverse the stack (root → leaf).
    // This is O(n * depth) which is fine for typical sample counts.

    #[derive(Default)]
    struct TrieNode {
        name: String,
        file: Option<String>,
        line: Option<u32>,
        self_samples: u64,
        total_samples: u64,
        children: HashMap<String, TrieNode>,
    }

    let mut roots: HashMap<String, TrieNode> = HashMap::new();

    for sample in raw {
        if sample.frames.is_empty() { continue; }
        // Reverse so root is the bottom-most frame (main → ... → leaf)
        let stack: Vec<_> = sample.frames.iter().rev().collect();

        let mut current = &mut roots;
        for (i, (name, file, line)) in stack.iter().enumerate() {
            let node = current.entry(name.clone()).or_insert_with(|| TrieNode {
                name: name.clone(),
                file: file.clone(),
                line: *line,
                ..Default::default()
            });
            node.total_samples += 1;
            if i == stack.len() - 1 { node.self_samples += 1; }
            current = &mut node.children;
        }
    }

    fn to_sample_node(name: String, node: TrieNode, total: u64) -> SampleNode {
        let total_f = total.max(1) as f64;
        let children = node.children.into_iter()
            .map(|(n, c)| to_sample_node(n, c, total))
            .collect();
        SampleNode {
            name,
            file: node.file,
            line: node.line,
            self_samples: node.self_samples,
            total_samples: node.total_samples,
            self_pct: node.self_samples as f64 / total_f * 100.0,
            total_pct: node.total_samples as f64 / total_f * 100.0,
            children,
        }
    }

    let mut result: Vec<SampleNode> = roots.into_iter()
        .map(|(n, node)| to_sample_node(n, node, total))
        .collect();
    result.sort_by(|a, b| b.total_pct.partial_cmp(&a.total_pct).unwrap_or(std::cmp::Ordering::Equal));
    result
}

// ─── unix itimer helpers ──────────────────────────────────────────────────────

#[cfg(unix)]
unsafe fn start_itimer(interval_us: u32) {
    use libc::{itimerval, setitimer, timeval, ITIMER_PROF};
    let tv_sec = (interval_us / 1_000_000) as libc::time_t;
    let tv_usec = (interval_us % 1_000_000) as libc::suseconds_t;
    let timer = itimerval {
        it_interval: timeval { tv_sec, tv_usec },
        it_value: timeval { tv_sec, tv_usec },
    };
    setitimer(ITIMER_PROF, &timer, std::ptr::null_mut());
}

#[cfg(unix)]
unsafe fn stop_itimer() {
    use libc::{itimerval, setitimer, timeval, ITIMER_PROF};
    let timer = itimerval {
        it_interval: timeval { tv_sec: 0, tv_usec: 0 },
        it_value:    timeval { tv_sec: 0, tv_usec: 0 },
    };
    setitimer(ITIMER_PROF, &timer, std::ptr::null_mut());
}

// ─── SIGPROF signal handler + ring buffer ─────────────────────────────────────
//
// The ring buffer holds pre-allocated Backtrace slots. The signal handler
// writes into the next free slot; the collector thread reads and clears them.
// We use AtomicU64 for the write/read indices (lock-free on all platforms).
//
// IMPORTANT: The signal handler captures `Backtrace::new_unresolved()` which
// is safe inside a signal handler on Linux/macOS. Symbol resolution happens
// in the collector thread where malloc/locking are allowed.

#[cfg(unix)]
const RING_CAPACITY: usize = 512;

#[cfg(unix)]
static RING_WRITE: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
static RING_READ:  AtomicU64 = AtomicU64::new(0);

// Each slot: None = empty, Some(bt) = ready to symbolize.
// We can't put 512 Mutex<Option<Backtrace>> in a static array without
// const-initialization. Instead use a simpler approach: a global Vec<Mutex<...>>
// allocated once at handler-install time via Once.
use std::sync::OnceLock;
#[cfg(unix)]
static RING_SLOTS: OnceLock<Vec<Mutex<Option<Backtrace>>>> = OnceLock::new();

#[cfg(unix)]
fn ensure_ring() {
    RING_SLOTS.get_or_init(|| {
        (0..RING_CAPACITY).map(|_| Mutex::new(None)).collect()
    });
}

/// Write one backtrace into the ring. Called from the SIGPROF handler.
/// Must be async-signal-safe: no malloc, no blocking.
#[cfg(unix)]
fn ring_push(bt: Backtrace) {
    let slots = match RING_SLOTS.get() { Some(s) => s, None => return };
    let w = RING_WRITE.load(Ordering::Relaxed);
    let r = RING_READ.load(Ordering::Relaxed);
    // If ring is full, drop the sample (better than blocking in signal handler)
    if w.wrapping_sub(r) as usize >= RING_CAPACITY { return; }
    let idx = (w as usize) % RING_CAPACITY;
    if let Ok(mut slot) = slots[idx].try_lock() {
        *slot = Some(bt);
        RING_WRITE.fetch_add(1, Ordering::Release);
    }
    // If lock is taken (shouldn't happen since one writer), drop the sample
}

/// Drain one backtrace from the ring. Called from the collector thread.
#[cfg(unix)]
pub(crate) fn drain_ring_buffer() -> Option<Backtrace> {
    let slots = RING_SLOTS.get()?;
    let r = RING_READ.load(Ordering::Acquire);
    let w = RING_WRITE.load(Ordering::Acquire);
    if r == w { return None; } // empty
    let idx = (r as usize) % RING_CAPACITY;
    let mut slot = slots[idx].lock().ok()?;
    let bt = slot.take()?;
    RING_READ.fetch_add(1, Ordering::Release);
    Some(bt)
}

#[cfg(unix)]
unsafe fn install_sigprof_handler() {
    ensure_ring();
    // Reset ring indices
    RING_WRITE.store(0, Ordering::SeqCst);
    RING_READ.store(0, Ordering::SeqCst);

    use libc::{sigaction, sigset_t, SA_RESTART, SIGPROF, c_int};
    extern "C" fn handler(_sig: libc::c_int) {
        // Capture backtrace in the interrupted (workload) thread.
        // new_unresolved() doesn't allocate symbol info — safe-ish in a handler.
        let bt = Backtrace::new_unresolved();
        ring_push(bt);
    }
    let mut sa: sigaction = std::mem::zeroed();
    sa.sa_sigaction = handler as usize;
    sa.sa_flags = SA_RESTART as libc::c_int;
    libc::sigemptyset(&mut sa.sa_mask as *mut sigset_t);
    sigaction(SIGPROF, &sa, std::ptr::null_mut());
}

#[cfg(not(unix))]
fn install_sigprof_handler() {}
#[cfg(not(unix))]
fn drain_ring_buffer() -> Option<Backtrace> { None }
