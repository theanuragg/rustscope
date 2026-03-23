//! Async profiling demo — Phase 2.
//!
//! Shows how RustScopeLayer integrates with tracing to measure async functions
//! correctly: active CPU time only, not suspension time.
//!
//! Run:
//!   cargo run --release --features async-profiling --bin async_demo
//!
//! Key thing to observe in async_demo.json → async_spans:
//!   simulate_io_latency:  active_time_ns << wall_time_ns  (suspended most of the time)
//!   compute_heavy:        active_time_ns ≈  wall_time_ns  (always on CPU)
//!   concurrent_worker:    first_poll_latency_ns shows executor queue pressure

use rustscope::Profiler;
use rustscope::allocator::TrackingAllocator;

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

#[cfg(feature = "async-profiling")]
use rustscope::async_profile;
#[cfg(feature = "async-profiling")]
use tracing_subscriber::prelude::*;

#[cfg(feature = "async-profiling")]
#[tokio::main]
async fn main() {
    // 1. Install RustScope layer BEFORE registry init
    async_profile::install();

    tracing_subscriber::registry()
        .with(async_profile::layer())
        .init();

    Profiler::init();

    // 2. Run async workload — all functions annotated with #[tracing::instrument]
    compute_heavy().await;
    simulate_io_latency().await;

    // 3. Spawn concurrent tasks — shows scheduler latency in first_poll_latency_ns
    let handles: Vec<_> = (0..8).map(|i| tokio::spawn(concurrent_worker(i))).collect();
    for h in handles { let _ = h.await; }

    // 4. Save — async_spans will appear in the JSON
    Profiler::print_summary();
    Profiler::save_json("async_demo.json").unwrap();
    println!("→ async_demo.json written");
    println!();
    println!("Interesting fields in async_spans[]:");
    println!("  active_time_ns.mean_ns        — actual CPU time per call");
    println!("  wall_time_ns.mean_ns           — total elapsed including .await");
    println!("  first_poll_latency_ns.mean_ns  — executor scheduling delay");
    println!("  total_poll_count               — how many times future was polled");
    println!("  total_suspension_count         — how many .await yield points hit");
}

#[cfg(not(feature = "async-profiling"))]
fn main() {
    eprintln!("This demo requires --features async-profiling");
    eprintln!("Run: cargo run --release --features async-profiling --bin async_demo");
}

// ─── Workload ─────────────────────────────────────────────────────────────────

#[cfg(feature = "async-profiling")]
#[tracing::instrument]
async fn compute_heavy() {
    // CPU-bound — active_time ≈ wall_time, low suspension_count
    let result = tokio::task::spawn_blocking(|| {
        let mut x = 0u64;
        for i in 0..5_000_000u64 {
            x = x.wrapping_add(i.wrapping_mul(6364136223846793005));
        }
        x
    }).await.unwrap();
    std::hint::black_box(result);
}

#[cfg(feature = "async-profiling")]
#[tracing::instrument]
async fn simulate_io_latency() {
    // I/O-heavy — active_time << wall_time, high suspension_count
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        let v: Vec<u32> = (0..500).collect(); // tiny CPU burst between sleeps
        std::hint::black_box(v);
    }
}

#[cfg(feature = "async-profiling")]
#[tracing::instrument(fields(id = id))]
async fn concurrent_worker(id: u32) {
    // Many instances — shows aggregated stats and multiple task_ids
    tokio::time::sleep(std::time::Duration::from_micros(id as u64 * 100)).await;
    let sum: u64 = (0..50_000).map(|i| (id as u64).wrapping_add(i)).sum();
    std::hint::black_box(sum);
}
