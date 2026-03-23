//! Profile demo — shows timing, CPU, stack, and call tree collection.
//!
//! Run:  cargo run --release --bin profile_demo
//! Out:  profile_demo.json  (open in any JSON viewer or jq)

use rustscope::{Profiler, profile, profile_scope, profile_block};
use rustscope::allocator::TrackingAllocator;

// Enable heap tracking (optional but recommended).
#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator;

fn main() {
    Profiler::init();

    run_workload();

    Profiler::print_summary();
    Profiler::save_json("profile_demo.json").expect("failed to write JSON");
    println!("→ profile_demo.json written");
}

fn run_workload() {
    profile_scope!("run_workload");

    bubble_sort_demo();
    string_allocation_demo();
    deep_call_chain(10);
    recursive_work(20);
}

// ── Example 1: #[profile] attribute ──────────────────────────────────────────

#[profile]
fn bubble_sort_demo() {
    let mut data: Vec<u32> = (0..8_000).rev().collect();
    for i in 0..data.len() {
        for j in 0..data.len() - 1 {
            if data[j] > data[j + 1] {
                data.swap(j, j + 1);
            }
        }
    }
    std::hint::black_box(data);
}

// ── Example 2: profile_scope! for inline functions ───────────────────────────

fn string_allocation_demo() {
    profile_scope!("string_allocation_demo");

    // profile_block! for one specific expression
    let strings = profile_block!("build_strings", {
        (0u32..20_000).map(|i| format!("item_{:06}", i)).collect::<Vec<_>>()
    });

    let total = profile_block!("measure_strings", {
        strings.iter().map(|s| s.len()).sum::<usize>()
    });

    std::hint::black_box(total);
}

// ── Example 3: call chain depth ──────────────────────────────────────────────

#[profile]
fn deep_call_chain(depth: u32) {
    if depth == 0 {
        leaf_work();
        return;
    }
    deep_call_chain(depth - 1);
}

#[profile]
fn leaf_work() {
    let sum: u64 = (0..500_000).sum();
    std::hint::black_box(sum);
}

// ── Example 4: recursion (shows max_recursion_depth field) ───────────────────

#[profile]
fn recursive_work(n: u64) -> u64 {
    if n == 0 { return 0; }
    n + recursive_work(n - 1)
}

// ── Example 5: profile_all on a module ───────────────────────────────────────

#[rustscope::profile_all]
mod math {
    pub fn square(x: f64) -> f64 { x * x }
    pub fn cube(x: f64) -> f64 { x * x * x }
    pub fn sum_squares(n: u32) -> f64 {
        (0..n).map(|i| square(i as f64)).sum()
    }
}
