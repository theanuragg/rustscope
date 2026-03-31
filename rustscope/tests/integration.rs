//! Integration tests for RustScope v3.
//!
//! Run: cargo test -p rustscope

use rustscope::{Profiler, ProfileGuard, assert_perf, profile, profile_scope, profile_block, profile_if};
use rustscope::features::{timeline, outliers, metadata};

// ─── helpers ─────────────────────────────────────────────────────────────────

// Serialize all integration tests to avoid shared-global-state races.
// Each test acquires this mutex; the guard is held for the test's lifetime.
use once_cell::sync::Lazy;
use std::sync::{Mutex, MutexGuard};
static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Reset all profiler state and serialize this test against all others.
/// Bind the return value: `let _guard = setup();`
fn setup() -> MutexGuard<'static, ()> {
    let guard: MutexGuard<'static, ()> = TEST_LOCK.lock()
        .unwrap_or_else(|p| p.into_inner());
    Profiler::init();
    Profiler::reset();
    timeline::disable();
    outliers::reset();
    outliers::enable_outlier_detection();
    outliers::set_outlier_threshold_sigma(3.0);
    metadata::reset();
    guard
}

// ─── Phase 1: core profiling ─────────────────────────────────────────────────

#[test]
fn test_basic_profiling() {
    let _guard = setup();
    
    #[profile]
    fn add(a: u32, b: u32) -> u32 { a + b }
    
    for i in 0..10 { std::hint::black_box(add(i, i + 1)); }
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "add")
        .expect("add should be recorded");
    
    assert_eq!(rec.call_count, 10);
    assert!(rec.timing.total_ns > 0, "total_ns should be nonzero");
    assert!(rec.timing.min_ns > 0,   "min_ns should be nonzero");
    assert!(rec.timing.max_ns >= rec.timing.min_ns, "max >= min");
    assert!(rec.timing.mean_ns > 0.0, "mean should be nonzero");
}

#[test]
fn test_profile_scope_macro() {
    let _guard = setup();
    
    fn work_with_scope() {
        profile_scope!("my_scope");
        std::hint::black_box(1u64.wrapping_mul(2));
    }
    
    for _ in 0..5 { work_with_scope(); }
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "my_scope")
        .expect("my_scope should be recorded");
    assert_eq!(rec.call_count, 5);
}

#[test]
fn test_profile_block_returns_value() {
    let _guard = setup();
    
    let result = profile_block!("computation", {
        (0u64..100).sum::<u64>()
    });
    
    assert_eq!(result, 4950, "profile_block must return the block value");
    
    let session = Profiler::collect();
    assert!(session.functions.iter().any(|f| f.name == "computation"),
        "computation scope should be recorded");
}

#[test]
fn test_call_count_accurate() {
    let _guard = setup();
    
    #[profile(name = "counter_fn")]
    fn counted() { std::hint::black_box(42u32); }
    
    let n = 50u64;
    for _ in 0..n { counted(); }
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "counter_fn").unwrap();
    assert_eq!(rec.call_count, n);
}

#[test]
fn test_recursion_depth() {
    let _guard = setup();
    
    #[profile(name = "recurse")]
    fn recurse(n: u32) -> u32 {
        if n == 0 { return 0; }
        recurse(n - 1) + 1
    }
    
    std::hint::black_box(recurse(10));
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "recurse").unwrap();
    assert!(rec.max_recursion_depth >= 10, 
        "should detect recursion depth >= 10, got {}", rec.max_recursion_depth);
}

#[test]
fn test_self_time_less_than_total() {
    let _guard = setup();
    
    #[profile(name = "outer")]
    fn outer() {
        inner();
        inner();
    }
    
    #[profile(name = "inner")]
    fn inner() {
        std::hint::black_box((0u64..1000).sum::<u64>());
    }
    
    for _ in 0..5 { outer(); }
    
    let session = Profiler::collect();
    let outer_rec = session.functions.iter().find(|f| f.name == "outer").unwrap();
    
    // Self time must be <= total time
    assert!(outer_rec.timing.self_ns <= outer_rec.timing.total_ns,
        "self_ns ({}) must be <= total_ns ({})",
        outer_rec.timing.self_ns, outer_rec.timing.total_ns);
}

#[test]
fn test_percentile_ordering() {
    let _guard = setup();
    
    #[profile(name = "percentile_test")]
    fn measured() {
        std::hint::black_box((0u64..10_000).sum::<u64>());
    }
    
    for _ in 0..200 { measured(); }
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "percentile_test").unwrap();
    
    assert!(rec.timing.p50_ns <= rec.timing.p95_ns, "p50 <= p95");
    assert!(rec.timing.p95_ns <= rec.timing.p99_ns, "p95 <= p99");
    assert!(rec.timing.min_ns <= rec.timing.p50_ns, "min <= p50");
    assert!(rec.timing.p99_ns <= rec.timing.max_ns, "p99 <= max");
}

// ─── Phase 1: reset ───────────────────────────────────────────────────────────

#[test]
fn test_reset_clears_records() {
    let _guard = setup();
    
    #[profile(name = "reset_test_fn")]
    fn measured() { std::hint::black_box(1u32); }
    
    for _ in 0..10 { measured(); }
    assert!(Profiler::collect().functions.iter().any(|f| f.name == "reset_test_fn"));
    
    Profiler::reset();
    let session = Profiler::collect();
    assert!(!session.functions.iter().any(|f| f.name == "reset_test_fn"),
        "records should be cleared after reset");
}

#[test]
fn test_profiling_works_after_reset() {
    let _guard = setup();
    
    #[profile(name = "after_reset")]
    fn measured() { std::hint::black_box(42u64); }
    
    measured();
    Profiler::reset();
    for _ in 0..7 { measured(); }
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "after_reset").unwrap();
    assert_eq!(rec.call_count, 7, "should only count calls after reset");
}

// ─── Phase 1: assert_perf! ────────────────────────────────────────────────────

#[test]
fn test_assert_perf_passes() {
    let _guard = setup();
    
    #[profile(name = "fast_fn")]
    fn fast() { std::hint::black_box(1u32 + 1); }
    
    for _ in 0..10 { fast(); }
    
    // Should not panic — 1s is a very generous threshold
    assert_perf!("fast_fn", total_ns < 1_000_000_000, "must be under 1 second");
}

#[test]
fn test_assert_perf_missing_fn_does_not_panic() {
    let _guard = setup();
    // assert_perf! on a function not in the session should NOT panic
    assert_perf!("nonexistent_fn", total_ns < 1_000, "n/a");
}

#[test]
#[should_panic(expected = "assert_perf FAILED")]
fn test_assert_perf_fails() {
    let _guard = setup();
    
    #[profile(name = "slow_fn_for_assert")]
    fn slow() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    slow();
    
    // 1 nanosecond threshold — must fail
    assert_perf!("slow_fn_for_assert", total_ns < 1, "impossible threshold");
}

// ─── profile_if! ─────────────────────────────────────────────────────────────

#[test]
fn test_profile_if_conditional() {
    let _guard = setup();
    
    fn run(condition: bool) {
        profile_if!(condition, "conditional_scope");
        std::hint::black_box(1u32);
    }
    
    run(true);
    run(true);
    run(false); // should not record
    
    let session = Profiler::collect();
    let rec = session.functions.iter().find(|f| f.name == "conditional_scope");
    match rec {
        Some(r) => assert_eq!(r.call_count, 2, "only 2 calls with condition=true"),
        None    => panic!("conditional_scope should appear (was called twice with true)"),
    }
}

// ─── v3 Feature: timeline ─────────────────────────────────────────────────────

#[test]
fn test_timeline_records_events() {
    let _guard = setup();
    timeline::enable();
    
    #[profile(name = "timeline_fn")]
    fn work() { std::hint::black_box((0u32..100).sum::<u32>()); }
    
    for _ in 0..5 { work(); }
    
    assert_eq!(timeline::event_count(), 5, "should record 5 events");
    
    let events = timeline::collect();
    assert!(events.iter().all(|e| e.name == "timeline_fn"));
    assert!(events.iter().all(|e| e.dur_ns > 0));
    
    // Timestamps should be monotonically non-decreasing
    let mut prev_t = 0u64;
    for e in &events {
        assert!(e.t >= prev_t, "timeline events should be non-decreasing in time");
        prev_t = e.t;
    }
}

#[test]
fn test_timeline_disabled_by_default() {
    let _guard = setup();
    // timeline is NOT enabled after setup()
    
    #[profile(name = "no_timeline_fn")]
    fn work() { std::hint::black_box(1u32); }
    
    for _ in 0..10 { work(); }
    
    assert_eq!(timeline::event_count(), 0, "should record nothing when disabled");
}

#[test]
fn test_timeline_reset_clears_events() {
    let _guard = setup();
    timeline::enable();
    
    #[profile(name = "reset_timeline_fn")]
    fn work() { std::hint::black_box(1u32); }
    
    work();
    assert_eq!(timeline::event_count(), 1);
    
    Profiler::reset(); // reset should also clear timeline
    assert_eq!(timeline::event_count(), 0, "reset should clear timeline events");
}

#[test]
fn test_timeline_save_ndjson() {
    let _guard = setup();
    timeline::enable();
    
    #[profile(name = "ndjson_fn")]
    fn work() { std::hint::black_box((0u64..1000).sum::<u64>()); }
    
    for _ in 0..3 { work(); }
    
    let path = "/tmp/rustscope_test_timeline.ndjson";
    timeline::save(path).expect("save should succeed");
    
    let content = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3, "should write 3 NDJSON lines");
    
    // Each line should be valid JSON
    for line in &lines {
        serde_json::from_str::<serde_json::Value>(line)
            .expect("each line should be valid JSON");
    }
    
    // Clean up
    let _ = std::fs::remove_file(path);
}

// ─── v3 Feature: outliers ─────────────────────────────────────────────────────

#[test]
fn test_outlier_detection_fires() {
    let _guard = setup();
    // Use 2σ threshold so the injected outlier is detected after fewer baseline calls
    outliers::set_outlier_threshold_sigma(2.0);
    
    #[profile(name = "outlier_test_fn")]
    fn measured(slow: bool) {
        if slow {
            std::thread::sleep(std::time::Duration::from_millis(50));
        } else {
            std::hint::black_box((0u64..10_000).sum::<u64>());
        }
    }
    
    // Build baseline — need at least 10 calls before detection kicks in
    for _ in 0..20 { measured(false); }
    
    // Inject a clear outlier — 50ms vs ~microseconds baseline
    measured(true);
    
    let detected = outliers::get_outliers();
    assert!(!detected.is_empty(), "should detect the 50ms outlier after 20-call baseline");
    assert!(detected.iter().any(|o| o.function == "outlier_test_fn"),
        "outlier should be for outlier_test_fn");
    assert!(detected[0].sigma > 2.0, "sigma should exceed threshold (2.0)");
}

#[test]
fn test_outlier_no_false_positives_with_few_calls() {
    let _guard = setup();
    
    #[profile(name = "few_calls_fn")]
    fn measured() { std::hint::black_box(1u32); }
    
    // Only 5 calls — not enough baseline, should detect nothing
    for _ in 0..5 { measured(); }
    
    let detected = outliers::get_outliers();
    assert!(detected.is_empty(),
        "should not detect outliers with < 10 baseline calls");
}

#[test]
fn test_budget_violation_fires() {
    let _guard = setup();
    
    #[profile(name = "budget_fn")]
    fn measured(slow: bool) {
        if slow {
            std::thread::sleep(std::time::Duration::from_millis(10));
        } else {
            std::hint::black_box(1u32);
        }
    }
    
    // 1ms budget
    outliers::set_budget("budget_fn", 1_000_000);
    
    measured(false); // fast — no violation
    measured(false);
    measured(true);  // 10ms — violates 1ms budget
    
    let violations = outliers::get_violations();
    assert!(!violations.is_empty(), "should record the budget violation");
    assert!(violations[0].exceeded_by_ns > 0, "exceeded_by should be positive");
    assert!(violations[0].exceeded_pct > 100.0, "exceeded by more than 100%");
}

#[test]
fn test_budget_callback_fires() {
    let _guard = setup();
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    
    let fired = Arc::new(AtomicBool::new(false));
    let fired2 = fired.clone();
    
    outliers::set_budget_with_callback("callback_budget_fn", 500_000, move |_v| {
        fired2.store(true, Ordering::SeqCst);
    });
    
    {
        let _g = ProfileGuard::enter("callback_budget_fn", file!(), line!(), module_path!());
        std::thread::sleep(std::time::Duration::from_millis(5)); // > 500µs budget
    }
    
    assert!(fired.load(Ordering::SeqCst), "budget callback should have fired");
}

// ─── v3 Feature: metadata ─────────────────────────────────────────────────────

#[test]
fn test_metadata_appears_in_session() {
    let _guard = setup();
    
    metadata::set("git_commit", "abc1234");
    metadata::tag("test-run");
    metadata::set_name("metadata_test");
    
    let session = Profiler::collect();
    let meta = session.session_meta.expect("session_meta should be present");
    
    let name = meta.get("session_name").and_then(|v| v.as_str());
    assert_eq!(name, Some("metadata_test"));
    
    let kv = meta.get("kv").and_then(|v| v.as_object()).unwrap();
    assert_eq!(kv.get("git_commit").and_then(|v| v.as_str()), Some("abc1234"));
    
    let tags = meta.get("tags").and_then(|v| v.as_array()).unwrap();
    assert!(tags.iter().any(|t| t.as_str() == Some("test-run")));
}

#[test]
fn test_metadata_reset_clears() {
    let _guard = setup();
    metadata::set("key", "value");
    assert!(!metadata::is_empty());
    
    metadata::reset();
    assert!(metadata::is_empty(), "reset should clear all metadata");
}

#[test]
fn test_metadata_tag_deduplication() {
    let _guard = setup();
    metadata::tag("duplicate");
    metadata::tag("duplicate");
    metadata::tag("duplicate");
    
    let m = metadata::get();
    assert_eq!(m.tags.iter().filter(|t| t.as_str() == "duplicate").count(), 1,
        "duplicate tags should be deduplicated");
}

// ─── v3 Feature: pprof export ─────────────────────────────────────────────────

#[test]
fn test_pprof_export_creates_file() {
    let _guard = setup();
    
    #[profile(name = "pprof_fn")]
    fn work() { std::hint::black_box((0u64..1000).sum::<u64>()); }
    
    for _ in 0..5 { work(); }
    
    let path = "/tmp/rustscope_test.pb.gz";
    Profiler::save_pprof(path).expect("pprof export should succeed");
    
    let bytes = std::fs::read(path).unwrap();
    // Check GZIP magic bytes: 0x1f 0x8b
    assert!(bytes.len() > 10, "pprof file should have content");
    assert_eq!(bytes[0], 0x1f, "should start with GZIP magic 0x1f");
    assert_eq!(bytes[1], 0x8b, "should start with GZIP magic 0x8b");
    
    let _ = std::fs::remove_file(path);
}

// ─── JSON serialization round-trip ───────────────────────────────────────────

#[test]
fn test_json_roundtrip() {
    let _guard = setup();
    
    #[profile(name = "roundtrip_fn")]
    fn work() { std::hint::black_box((0u64..100).sum::<u64>()); }
    
    for _ in 0..3 { work(); }
    
    let path = "/tmp/rustscope_roundtrip.json";
    Profiler::save_json(path).expect("save_json should succeed");
    
    // Deserialize
    let json = std::fs::read_to_string(path).unwrap();
    let session: rustscope::ProfileSession = serde_json::from_str(&json)
        .expect("saved JSON should deserialize cleanly");
    
    assert_eq!(session.schema_version, 3, "schema version should be 3");
    assert!(session.functions.iter().any(|f| f.name == "roundtrip_fn"));
    
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_session_has_host_info() {
    let _guard = setup();
    let session = Profiler::collect();
    assert!(!session.host.os.is_empty(),   "os should be set");
    assert!(!session.host.arch.is_empty(), "arch should be set");
}
