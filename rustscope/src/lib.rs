//! # RustScope v3
//!
//! Function-level profiler and micro-benchmarker for Rust.
//! **Outputs structured JSON only** — no rendering, no HTML.
//!
//! ## Feature flags
//!
//! | Flag              | What it adds                                         |
//! |-------------------|------------------------------------------------------|
//! | `hw-counters`     | Linux perf_event: CPU cycles, cache misses, branches |
//! | `async-profiling` | tracing-subscriber Layer for async span tracking     |
//! | `sampling`        | SIGPROF-based sampling profiler (no annotations)     |
//! | `full`            | All of the above                                     |

pub use rustscope_macros::{benchmark, profile, profile_all};

// ── Core (Phase 1) ────────────────────────────────────────────────────────────
pub mod allocator;
pub mod output;

mod collectors;
mod global;
mod stats;

pub use global::{ProfileGuard, Profiler, GLOBAL_PROFILER};
pub use stats::run_benchmark;
pub use output::schema::{
    BenchmarkRecord, CallTreeNode, CpuCounters, FunctionRecord,
    MemoryMetrics, ProfileSession, StackMetrics, TimingStats,
};

// ── Phase 2: Async profiling ──────────────────────────────────────────────────
#[cfg(feature = "async-profiling")]
pub mod async_profile;

// ── Phase 3: Sampling profiler ────────────────────────────────────────────────
#[cfg(feature = "sampling")]
pub mod sampling;

// ── Phase 4: Diff + export + threads ─────────────────────────────────────────
pub mod diff;
pub mod export;
pub mod thread_profile;

// ── v3 Features: timeline, outliers, metadata, pprof ─────────────────────────
pub mod features;

/// Session metadata API — git commit, CI tags, custom key-value pairs.
pub use features::metadata;

/// Outlier detection and latency-budget (SLO) API.
pub use features::outliers;

/// Per-call timeline event log (NDJSON output).
pub use features::timeline;

/// pprof protobuf export (go tool pprof / Pyroscope compatible).
pub use features::pprof_export;

// ── Macros ────────────────────────────────────────────────────────────────────

/// Instrument a named scope inline.
/// The guard lives until end of the enclosing scope.
/// Use `let _guard = …` — NOT `let _ = …` (Rust drops `_` immediately).
#[macro_export]
macro_rules! profile_scope {
    ($name:literal) => {
        let __rs_scope_guard = $crate::ProfileGuard::enter(
            $name, file!(), line!(), module_path!()
        );
        let _ = &__rs_scope_guard;
    };
}

/// Profile a block expression and return its value.
/// The guard is dropped at the end of the inner block.
///
/// ```rust,ignore
/// let result = profile_block!("sort_phase", {
///     let mut v = data.clone();
///     v.sort_unstable();
///     v
/// });
/// ```
#[macro_export]
macro_rules! profile_block {
    ($name:literal, $block:block) => {{
        let __rs_blk_guard = $crate::ProfileGuard::enter(
            $name, file!(), line!(), module_path!()
        );
        let __rs_blk_val = { $block };
        drop(__rs_blk_guard);
        __rs_blk_val
    }};
}

/// Assert a performance threshold. Panics with a clear message if violated.
/// Silently passes when the function wasn't recorded (profiler disabled, etc.).
///
/// ```rust,ignore
/// assert_perf!("parse_config", total_ns < 1_000_000, "must parse in < 1ms");
/// assert_perf!("render",       p99_ns   < 5_000_000, "p99 must be < 5ms");
/// ```
#[macro_export]
macro_rules! assert_perf {
    ($fn_name:expr, $metric:ident $op:tt $threshold:expr, $msg:expr) => {{
        let __rs_session = $crate::Profiler::collect();
        if let Some(__rs_f) = __rs_session.functions.iter().find(|f| f.name == $fn_name) {
            let __rs_val = __rs_f.timing.$metric;
            if !(__rs_val $op $threshold) {
                panic!(
                    "[rustscope] assert_perf FAILED: `{}` — {} {} {} \
                     (actual: {}ns) — {}",
                    $fn_name,
                    stringify!($metric), stringify!($op), $threshold,
                    __rs_val, $msg,
                );
            }
        }
    }};
}

/// Conditionally profile a scope. Zero overhead when `$cond` is false.
/// The `Option<ProfileGuard>` lives until end of the enclosing scope.
///
/// ```rust,ignore
/// let verbose = std::env::var("PROFILE").is_ok();
/// profile_if!(verbose, "expensive_section");
/// ```
#[macro_export]
macro_rules! profile_if {
    ($cond:expr, $name:literal) => {
        let __rs_if_guard: Option<$crate::ProfileGuard> = if $cond {
            Some($crate::ProfileGuard::enter($name, file!(), line!(), module_path!()))
        } else {
            None
        };
        let _ = &__rs_if_guard;
    };
}
