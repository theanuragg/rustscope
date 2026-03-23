//! Memory profiling demo — shows per-function heap allocation tracking.
//!
//! Run:  cargo run --release --bin memory_demo
//! Out:  memory_demo.json  — look at the `memory` field in each function record.
//!
//! Key JSON fields:
//!   memory.total_alloc_bytes     — total bytes allocated across all calls
//!   memory.total_dealloc_bytes   — total bytes freed
//!   memory.net_retained_bytes    — bytes still live after function returns
//!   memory.peak_delta_bytes      — peak heap above entry-level in one call
//!   memory.mean_alloc_per_call   — average bytes allocated per invocation
//!   memory.alloc_op_count        — total number of allocator calls

use rustscope::{Profiler, profile, profile_scope};
use rustscope::allocator::TrackingAllocator;

// REQUIRED for memory tracking.
#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    Profiler::init();

    allocation_patterns();
    cache_friendly_vs_unfriendly();
    string_intern_simulation();

    Profiler::print_summary();
    Profiler::save_json("memory_demo.json").expect("failed to write JSON");
    println!("→ memory_demo.json written");
}

fn allocation_patterns() {
    profile_scope!("allocation_patterns");

    for _ in 0..10 {
        // Compare: pre-allocated vs growing vec
        preallocated_vec(10_000);
        growing_vec(10_000);
        // String arena vs many small strings
        small_strings_demo(500);
        single_large_string(500);
    }
}

#[profile]
fn preallocated_vec(n: usize) -> Vec<u64> {
    // One allocation: with_capacity avoids reallocations
    let mut v = Vec::with_capacity(n);
    for i in 0..n { v.push(i as u64 * 7); }
    v
}

#[profile]
fn growing_vec(n: usize) -> Vec<u64> {
    // Multiple reallocations as vec grows
    let mut v = Vec::new();
    for i in 0..n { v.push(i as u64 * 7); }
    v
}

#[profile]
fn small_strings_demo(count: usize) -> Vec<String> {
    // Many small heap objects — high alloc_op_count
    (0..count).map(|i| format!("key_{:04}", i)).collect()
}

#[profile]
fn single_large_string(count: usize) -> String {
    // One big allocation vs many small ones
    let mut s = String::with_capacity(count * 8);
    for i in 0..count {
        s.push_str(&format!("key_{:04}", i));
    }
    s
}

// ── Cache-friendly vs cache-unfriendly access patterns ───────────────────────

fn cache_friendly_vs_unfriendly() {
    profile_scope!("cache_friendly_vs_unfriendly");
    let n = 1024;
    for _ in 0..5 {
        row_major_sum(n);
        col_major_sum(n);
    }
}

#[profile]
fn row_major_sum(n: usize) -> u64 {
    // Sequential memory access — cache-friendly
    let matrix: Vec<Vec<u64>> = (0..n).map(|i|
        (0..n).map(|j| (i * n + j) as u64).collect()
    ).collect();
    matrix.iter().flatten().sum()
}

#[profile]
fn col_major_sum(n: usize) -> u64 {
    // Strided memory access — cache-unfriendly (higher miss rate)
    let matrix: Vec<Vec<u64>> = (0..n).map(|i|
        (0..n).map(|j| (i * n + j) as u64).collect()
    ).collect();
    (0..n).map(|j| (0..n).map(|i| matrix[i][j]).sum::<u64>()).sum()
}

// ── String interning simulation ───────────────────────────────────────────────

fn string_intern_simulation() {
    profile_scope!("string_intern_simulation");
    // Shows: net_retained_bytes will be > 0 because the HashMap stays alive
    let _intern_table = build_intern_table(5_000);
    // After this scope, _intern_table is dropped → net_retained_bytes ~ 0 in
    // the parent scope.
}

#[profile]
fn build_intern_table(n: usize) -> std::collections::HashMap<String, u32> {
    let mut map = std::collections::HashMap::with_capacity(n);
    for i in 0..n {
        map.insert(format!("symbol_{:06}", i), i as u32);
    }
    map
}
