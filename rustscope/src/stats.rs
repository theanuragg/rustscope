//! Statistical micro-benchmark runner.
//!
//! Called by the `#[benchmark]` macro. Runs `f` for `warmup` iterations
//! (discarded), then `iters` measured iterations. Computes full statistics
//! and emits a `BenchmarkRecord` to the global store.

use std::hint::black_box;
use std::time::Instant;

use crate::collectors::{
    memory,
    cpu::CpuCounterGuard,
    timing::TimingAccumulator,
};
use crate::output::schema::{BenchmarkRecord, MemoryMetrics};
use crate::global::GLOBAL_PROFILER;

/// Run a benchmark closure and record results.
///
/// Called by `#[benchmark]` — you shouldn't need to call this directly,
/// but it's public so you can use it without the macro.
///
/// ```rust
/// rustscope::run_benchmark(
///     "my_bench", file!(), line!(), module_path!(),
///     1000, 100,
///     || { std::hint::black_box(my_fn()); }
/// );
/// ```
pub fn run_benchmark<F>(
    name: &str,
    file: &'static str,
    line: u32,
    module: &'static str,
    iters: u64,
    warmup: u64,
    mut f: F,
) where
    F: FnMut(),
{
    // ── warmup ────────────────────────────────────────────────────────────────
    for _ in 0..warmup {
        black_box(f());
    }

    // ── measured runs ─────────────────────────────────────────────────────────
    let mut timing = TimingAccumulator::new();
    let mut raw_samples: Vec<u64> = Vec::with_capacity(iters.min(10_000) as usize);

    let alloc_start = memory::snapshot();
    let mut total_alloc:   u64 = 0;
    let mut total_dealloc: u64 = 0;
    let mut total_ops:     u64 = 0;
    let mut peak_delta:    u64 = 0;

    let session_start = Instant::now();

    for i in 0..iters {
        let alloc_before = memory::snapshot();
        let t0 = Instant::now();

        black_box(f());

        let elapsed = t0.elapsed().as_nanos() as u64;
        let alloc_after = memory::snapshot();

        timing.record(elapsed, elapsed); // self_ns == total_ns for leaf benchmarks
        if raw_samples.len() < 10_000 {
            raw_samples.push(elapsed);
        }

        total_alloc   += alloc_after.alloc_delta(&alloc_before);
        total_dealloc += alloc_after.dealloc_delta(&alloc_before);
        total_ops     += alloc_after.ops_delta(&alloc_before);
        let pd = alloc_after.peak_delta(&alloc_before);
        if pd > peak_delta { peak_delta = pd; }
    }

    let session_ns = session_start.elapsed().as_nanos() as u64;

    // ── CPU counters for the entire benchmark run ─────────────────────────────
    // (Single pass with counters open — less accurate per-iteration but cheap)
    let cpu = {
        let g = CpuCounterGuard::open();
        for _ in 0..iters.min(100) {
            black_box(f());
        }
        g.read()
    };

    // ── build record ──────────────────────────────────────────────────────────
    let throughput = if timing.mean > 0.0 {
        1_000_000_000.0 / timing.mean
    } else { 0.0 };

    let memory = if memory::ALLOCATOR_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
        Some(MemoryMetrics {
            total_alloc_bytes:   total_alloc,
            total_dealloc_bytes: total_dealloc,
            net_retained_bytes:  total_alloc as i64 - total_dealloc as i64,
            peak_delta_bytes:    peak_delta as i64,
            alloc_count: total_ops,
            dealloc_count: 0,
            mean_alloc_per_call: if iters > 0 { total_alloc as f64 / iters as f64 } else { 0.0 },
            alloc_op_count:      total_ops,
        })
    } else { None };

    let rec = BenchmarkRecord {
        name: name.to_owned(),
        iterations: iters,
        total_ns: session_ns,
        avg_ns: if iters > 0 { session_ns / iters } else { 0 },
        memory,
        file: file.to_owned(),
        line,
        warmup_iterations: warmup,
        throughput_per_sec: throughput,
        timing: timing.build_stats(session_ns),
        cpu,
        raw_samples_ns: if iters <= 10_000 { Some(raw_samples) } else { None },
    };

    GLOBAL_PROFILER.push_benchmark(rec);
    println!("[rustscope bench] {name}: median={} p99={} throughput={:.0}/s",
        crate::output::writer::fmt_ns(timing.percentiles().0),
        crate::output::writer::fmt_ns(timing.percentiles().2),
        throughput,
    );
}
