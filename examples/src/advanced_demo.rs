//! Advanced demo — single JSON report with code overview.
//!
//! Run:
//!   cargo run --release --features "async-profiling sampling" --bin advanced_demo
//!
//! This will:
//!   - run a mix of sync, multithreaded, and sampling workloads
//!   - write ONE `advanced_demo.json` file with a full overview of the code
//!   - print a human summary to stdout

use rustscope::{Profiler, profile, profile_scope};
use rustscope::allocator::TrackingAllocator;

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    // Run everything once and emit a single JSON report + stdout overview.
    Profiler::run_with_json("advanced_demo.json", || {
        // ── instrumentation profiling ─────────────────────────────────────────
        run_sync_workload();
        run_multithreaded();

        // ── sampling profiler (no instrumentation needed) ────────────────────
        #[cfg(feature = "sampling")]
        {
            use rustscope::sampling::{SamplingProfiler, SamplingConfig};
            let config = SamplingConfig {
                frequency_hz: 200,
                max_stack_depth: 24,
                ..Default::default()
            };
            let (_, _sample_report) = SamplingProfiler::profile(config, || {
                // Run workload without any #[profile] annotations
                uninstrumented_heavy_work();
            });
        }
    }).unwrap();

    // Human-readable overview of the run printed once.
    let session = rustscope::Profiler::collect();
    rustscope::output::writer::print_summary(&session);
    println!("→ advanced_demo.json written (single report)");
}

// ── sync workload ─────────────────────────────────────────────────────────────

fn run_sync_workload() {
    profile_scope!("run_sync_workload");
    cpu_bound_sort();
    memory_bound_strings();
}

#[profile]
fn cpu_bound_sort() {
    let mut v: Vec<u32> = (0..50_000).rev().collect();
    v.sort();
    std::hint::black_box(v);
}

#[profile]
fn memory_bound_strings() {
    let strings: Vec<String> = (0..10_000)
        .map(|i| format!("string_{:08}", i))
        .collect();
    std::hint::black_box(strings.len());
}

// ── multithreaded workload ────────────────────────────────────────────────────

fn run_multithreaded() {
    profile_scope!("run_multithreaded");
    let handles: Vec<_> = (0..4).map(|i| {
        std::thread::Builder::new()
            .name(format!("worker-{}", i))
            .spawn(move || {
                worker_task(i, 10_000);
            })
            .unwrap()
    }).collect();
    for h in handles { h.join().unwrap(); }
}

#[profile]
fn worker_task(id: u32, n: u32) {
    let sum: u64 = (0..n).map(|i| (id as u64).wrapping_mul(i as u64)).sum();
    std::hint::black_box(sum);
}

// ── uninstrumented code (for sampling demo) ───────────────────────────────────

fn uninstrumented_heavy_work() {
    // No #[profile] here — sampling profiler finds this anyway
    let mut matrix = vec![0f64; 512 * 512];
    for i in 0..512 {
        for j in 0..512 {
            matrix[i * 512 + j] = (i as f64 * j as f64).sqrt();
        }
    }
    std::hint::black_box(matrix);
}
