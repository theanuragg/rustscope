//! Features demo — single JSON report + code overview.
//!
//! Demonstrates all features added beyond Phase 1:
//!   - Timeline: per-call event log (every single invocation)
//!   - Outlier detection: calls > 3σ above mean flagged automatically
//!   - Latency budgets: register a ns threshold, get violation reports
//!   - Session metadata: git commit, CI tags, custom KV
//!   - pprof export: go tool pprof / Pyroscope / Grafana compatible
//!   - Thread profiling: per-thread breakdown, load imbalance
//!
//! Run:
//!   cargo run --release --bin features_demo
//!
//! This will:
//!   - enable timeline, outliers, budgets, metadata, thread profiling
//!   - run a variety of workloads once
//!   - write ONE `features_demo.json` file with a rich overview
//!   - print a concise summary to stdout

use rustscope::{
    Profiler, profile, profile_scope, profile_block, benchmark,
    timeline, outliers, metadata,
};
use rustscope::allocator::TrackingAllocator;

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    // Run everything once and emit a single JSON report + stdout overview.
    Profiler::run_with_json("features_demo.json", || {
        // ── 1. Init-like configuration ───────────────────────────────────────
        Profiler::detect_ci();
        metadata::set_name("features_demo run");
        metadata::set_description("Demonstrating all RustScope v3 features");
        metadata::set("cargo_profile", if cfg!(debug_assertions) { "debug" } else { "release" });
        metadata::tag("example");
        metadata::tag("features");

        timeline::enable();
        timeline::set_max_events(500_000);

        Profiler::set_budget("sort_data", 5_000_000);
        outliers::set_budget_with_callback("risky_io_sim", 2_000_000, |v| {
            eprintln!("[BUDGET] risky_io_sim exceeded by {:.1}% (took {}ns, budget {}ns)",
                v.exceeded_pct, v.duration_ns, v.budget_ns);
        });
        outliers::set_outlier_threshold_sigma(2.5);

        // ── 2. Run workloads ─────────────────────────────────────────────────
        println!("Running workload...");
        for _ in 0..20 {
            sort_data(2_000);
        }
        sort_data(50_000);
        sort_data(100_000);

        for _ in 0..15 {
            risky_io_sim(false);
        }
        risky_io_sim(true);

        run_multithreaded();
        process_strings();
        recursive_fib(20);
        allocation_heavy();

        let x = profile_block!("inline_computation", {
            (0u64..100_000).map(|i| i.wrapping_mul(6364136223846793005)).sum::<u64>()
        });
        std::hint::black_box(x);
    }).unwrap();

    // Print a concise overview using the single saved session.
    let session = rustscope::Profiler::collect();
    rustscope::output::writer::print_summary(&session);
    println!("→ features_demo.json written (single report)");

}

// ── Workload functions ────────────────────────────────────────────────────────

#[profile]
fn sort_data(n: usize) {
    let mut v: Vec<u32> = (0..n as u32).rev().collect();
    v.sort_unstable();
    std::hint::black_box(v);
}

#[profile]
fn risky_io_sim(slow: bool) {
    // Simulates occasionally slow I/O
    let work = if slow { 5_000_000u32 } else { 100_000 };
    let sum: u64 = (0..work).map(|i| i as u64).sum();
    std::hint::black_box(sum);
}

fn run_multithreaded() {
    profile_scope!("multithreaded_section");
    let handles: Vec<_> = (0..4).map(|i| {
        std::thread::Builder::new()
            .name(format!("worker-{i}"))
            .spawn(move || worker(i, 100_000))
            .unwrap()
    }).collect();
    for h in handles { h.join().unwrap(); }
}

#[profile]
fn worker(id: u32, n: u32) {
    let sum: u64 = (0..n).map(|i| (id as u64).wrapping_mul(i as u64)).sum();
    std::hint::black_box(sum);
}

#[profile]
fn process_strings() {
    let strings: Vec<String> = (0..5_000)
        .map(|i| format!("item_{:08x}", i))
        .collect();
    let joined = strings.join(",");
    std::hint::black_box(joined.len());
}

#[profile]
fn recursive_fib(n: u32) -> u64 {
    if n <= 1 { return n as u64; }
    // Not optimal — intentional recursion to test recursion_depth tracking
    recursive_fib(n - 1) + recursive_fib(n - 2)
}

#[profile]
fn allocation_heavy() {
    let mut v: Vec<Vec<u8>> = Vec::new();
    for i in 0..100 {
        v.push(vec![i as u8; 1024]);  // 100 × 1KB allocations
    }
    std::hint::black_box(v.len());
}

// ── Benchmark (appears in profile.json under "benchmarks") ───────────────────

#[benchmark(iters = 500, warmup = 50)]
fn bench_sort_small() {
    let mut v: Vec<u32> = (0..100).rev().collect();
    v.sort_unstable();
    std::hint::black_box(v);
}

#[benchmark(iters = 200, warmup = 20)]
fn bench_sort_large() {
    let mut v: Vec<u32> = (0..10_000).rev().collect();
    v.sort_unstable();
    std::hint::black_box(v);
}
