//! Benchmark demo — statistical micro-benchmarking like Criterion but simpler.
//!
//! Run:  cargo run --release --bin benchmark_demo
//! Out:  benchmark_demo.json

use rustscope::{Profiler, benchmark, run_benchmark};
use rustscope::allocator::TrackingAllocator;

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    Profiler::init();

    // Method 1: #[benchmark] attribute (runs the function in a loop).
    bench_sort_stdlib();
    bench_sort_bubble();
    bench_string_format();

    // Method 2: run_benchmark() directly — useful for comparing variants.
    let data_small: Vec<u32> = (0..100).rev().collect();
    let data_large: Vec<u32> = (0..10_000).rev().collect();

    run_benchmark(
        "vec_sort/n=100", file!(), line!(), module_path!(),
        5_000, 200,
        || { let mut v = data_small.clone(); v.sort(); std::hint::black_box(v); }
    );

    run_benchmark(
        "vec_sort/n=10000", file!(), line!(), module_path!(),
        500, 50,
        || { let mut v = data_large.clone(); v.sort(); std::hint::black_box(v); }
    );

    run_benchmark(
        "hash_map_insert/1000_entries", file!(), line!(), module_path!(),
        2_000, 100,
        || {
            let mut map = std::collections::HashMap::with_capacity(1024);
            for i in 0u32..1000 {
                map.insert(i, i * 2);
            }
            std::hint::black_box(map);
        }
    );

    Profiler::print_summary();
    Profiler::save_json("benchmark_demo.json").expect("failed to write JSON");
    println!("→ benchmark_demo.json written");
}

// ── #[benchmark] macro examples ───────────────────────────────────────────────

/// Default: 1000 measured iterations, 100 warmup.
#[benchmark]
fn bench_sort_stdlib() {
    let mut data: Vec<u32> = (0..1_000).rev().collect();
    data.sort();
    std::hint::black_box(data);
}

#[benchmark(iters = 500, warmup = 50)]
fn bench_sort_bubble() {
    let mut data: Vec<u32> = (0..200).rev().collect();
    // Intentionally slow for comparison
    for i in 0..data.len() {
        for j in 0..data.len() - 1 {
            if data[j] > data[j + 1] { data.swap(j, j + 1); }
        }
    }
    std::hint::black_box(data);
}

#[benchmark(iters = 2000, warmup = 200)]
fn bench_string_format() {
    let s = format!("hello_{:08x}_{:.4}", 0xdeadbeefu32, std::f64::consts::PI);
    std::hint::black_box(s);
}
