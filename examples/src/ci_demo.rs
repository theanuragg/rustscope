//! CI regression detection demo — Phase 4.
//!
//! Simulates a two-run CI workflow:
//!   Run 1 (baseline): save profile to baseline.json
//!   Run 2 (current):  run with a deliberately slower function, then diff
//!
//! Run:
//!   cargo run --release --bin ci_demo
//!
//! Outputs:
//!   baseline.json        — first run profile
//!   current.json         — second run profile (with regression)
//!   diff_report.json     — structured diff
//!   current.chrome_trace.json, current.speedscope.json, current.csv
//!
//! In a real CI pipeline you would:
//!   1. Store baseline.json as a CI artifact or in git
//!   2. Run `cargo run --bin my_app` to produce current.json
//!   3. Run the diff and exit(1) if has_critical_regressions()

use rustscope::{
    Profiler, profile, profile_scope, benchmark, run_benchmark,
    diff::{SessionDiff, DiffConfig},
    export::{export, ExportFormat},
    thread_profile::ThreadProfiler,
};
use rustscope::allocator::TrackingAllocator;

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    // ── Run 1: Baseline ───────────────────────────────────────────────────────
    println!("=== Run 1: Baseline ===");
    {
        Profiler::init();
        ThreadProfiler::enable();

        workload_v1();                              // fast implementation
        bench_sort_baseline();
        bench_string_baseline();

        Profiler::save_json("baseline.json").unwrap();
        println!("→ baseline.json written");
    }

    // ── Run 2: Current (with a regression introduced) ─────────────────────────
    println!("\n=== Run 2: Current (regression injected) ===");
    {
        // Reset global state for second run
        // In a real pipeline this would be a separate process invocation.
        // Here we demonstrate by calling save_json again after more work.

        workload_v2_with_regression();             // intentionally slow version
        bench_sort_baseline();
        bench_string_baseline();

        Profiler::save_json("current.json").unwrap();
        println!("→ current.json written");
    }

    // ── Phase 4: Diff ─────────────────────────────────────────────────────────
    println!("\n=== Diff: baseline vs current ===");

    let baseline = SessionDiff::load_session("baseline.json")
        .expect("baseline.json not found");
    let current = SessionDiff::load_session("current.json")
        .expect("current.json not found");

    let config = DiffConfig {
        regression_threshold_pct: 10.0,
        improvement_threshold_pct: 5.0,
        noise_floor_ns: 1_000,
        check_memory: true,
        min_call_count: 1,
        ..Default::default()
    };

    let diff = SessionDiff::compare(&baseline, &current, &config, "baseline.json", "current.json");
    diff.save_json("diff_report.json").unwrap();
    println!("→ diff_report.json written");
    println!();
    println!("{}", diff.summary_text());

    // CI gate: exit 1 on critical regressions
    if diff.has_critical_regressions() {
        eprintln!("\n[FAIL] Critical performance regressions detected!");
        eprintln!("       In CI: exit(1) here to fail the build.");
        // std::process::exit(1);  // uncomment for real CI use
    } else if diff.has_any_regressions() {
        eprintln!("\n[WARN] Minor/moderate regressions detected.");
    } else {
        println!("[OK] No regressions.");
    }

    // ── Phase 4: Export current session in all formats ─────────────────────────
    println!("\n=== Exporting formats ===");
    let session = Profiler::collect();

    export(&session, ExportFormat::ChromeTrace,     "current.chrome_trace.json").unwrap();
    export(&session, ExportFormat::SpeedScope,       "current.speedscope.json").unwrap();
    export(&session, ExportFormat::CollapsedStacks, "current.stacks.txt").unwrap();
    export(&session, ExportFormat::Csv,             "current.csv").unwrap();
    println!("→ chrome://tracing  : current.chrome_trace.json");
    println!("→ speedscope.app    : current.speedscope.json");
    println!("→ flamegraph.pl     : current.stacks.txt");
    println!("→ pandas/Excel      : current.csv");

    // ── assert_perf! macro — inline performance assertions ────────────────────
    println!("\n=== Performance assertions ===");
    // These will panic if the thresholds are violated.
    // Use in test suites with #[test] to get CI-integrated perf gates.
    //
    // assert_perf!("fast_fn", total_ns < 1_000_000, "must finish in < 1ms");
    // assert_perf!("fast_fn", p99_ns < 2_000_000,   "p99 must be < 2ms");
    println!("(assert_perf! examples shown in comments — uncomment to enforce thresholds)");

    // ── Thread breakdown ───────────────────────────────────────────────────────
    ThreadProfiler::save_json("thread_profile.json").unwrap();
    println!("\n→ thread_profile.json written");
    println!("  Contains per-thread timing + load_imbalance coefficient");
    println!("  load_imbalance = 0.0 means perfect balance across threads");
}

// ─── Workload v1 (fast — baseline) ───────────────────────────────────────────

fn workload_v1() {
    profile_scope!("workload");
    fast_sort(5_000);
    fast_hash(1_000);
    for _ in 0..4 {
        std::thread::spawn(|| worker_fn(10_000)).join().unwrap();
    }
}

#[profile]
fn fast_sort(n: usize) {
    let mut v: Vec<u32> = (0..n as u32).rev().collect();
    v.sort_unstable();
    std::hint::black_box(v);
}

#[profile]
fn fast_hash(n: usize) {
    let mut m = std::collections::HashMap::with_capacity(n);
    for i in 0..n { m.insert(i, i * 2); }
    std::hint::black_box(m);
}

#[profile]
fn worker_fn(n: u32) {
    let sum: u64 = (0..n).map(|i| i as u64).sum();
    std::hint::black_box(sum);
}

// ─── Workload v2 (slow — regression injected) ────────────────────────────────

fn workload_v2_with_regression() {
    profile_scope!("workload");
    slow_sort_regression(5_000);  // bubble sort instead of std sort — intentional regression
    fast_hash(1_000);
    for _ in 0..4 {
        std::thread::spawn(|| worker_fn(10_000)).join().unwrap();
    }
}

#[profile]
fn slow_sort_regression(n: usize) {
    // Intentionally O(n²) — will show up as a critical regression in the diff
    let mut v: Vec<u32> = (0..n as u32).rev().collect();
    for i in 0..v.len() {
        for j in 0..v.len() - 1 {
            if v[j] > v[j + 1] { v.swap(j, j + 1); }
        }
    }
    std::hint::black_box(v);
}

// ─── Benchmarks ───────────────────────────────────────────────────────────────

#[benchmark(iters = 500, warmup = 50)]
fn bench_sort_baseline() {
    let mut v: Vec<u32> = (0..1_000).rev().collect();
    v.sort_unstable();
    std::hint::black_box(v);
}

#[benchmark(iters = 1000, warmup = 100)]
fn bench_string_baseline() {
    let s = format!("bench_{:08x}", 0xdeadbeef_u32);
    std::hint::black_box(s);
}
