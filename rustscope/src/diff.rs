//! # Phase 4a: Session Diff — Regression Detection
//!
//! Compares two `ProfileSession` JSON files and produces a structured diff:
//! which functions got faster, which regressed, which are new or removed.
//!
//! This is the core of CI regression detection.
//!
//! ## Usage
//!
//! ```rust
//! use rustscope::diff::{SessionDiff, DiffConfig, RegressionSeverity};
//!
//! // Load two sessions (baseline vs current)
//! let baseline = SessionDiff::load_session("baseline.json")?;
//! let current  = SessionDiff::load_session("current.json")?;
//!
//! let config = DiffConfig {
//!     regression_threshold_pct: 10.0,  // > 10% slower = regression
//!     improvement_threshold_pct: 5.0,  // > 5% faster = improvement
//!     noise_floor_ns: 1_000,           // ignore changes < 1µs (noise)
//!     ..Default::default()
//! };
//!
//! let diff = SessionDiff::compare(&baseline, &current, &config);
//! diff.save_json("diff.json")?;
//!
//! // CI: exit code 1 if any critical regressions
//! if diff.has_critical_regressions() {
//!     eprintln!("{}", diff.summary_text());
//!     std::process::exit(1);
//! }
//! ```
//!
//! ## CI GitHub Actions integration
//!
//! ```yaml
//! - run: cargo run --bin my_app
//! - run: |
//!     # Compare current run to stored baseline
//!     rustscope-cli diff baseline.json profile.json \
//!       --threshold 15 \
//!       --fail-on critical
//! ```

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::output::schema::{FunctionRecord, ProfileSession, BenchmarkRecord};

// ─── configuration ────────────────────────────────────────────────────────────

/// Thresholds controlling what counts as a regression or improvement.
#[derive(Debug, Clone)]
pub struct DiffConfig {
    /// % change in `metric` before it's called a regression. Default: 10.0.
    pub regression_threshold_pct: f64,
    /// % change before it's called an improvement. Default: 5.0.
    pub improvement_threshold_pct: f64,
    /// Absolute change (ns) below which changes are ignored as noise. Default: 500.
    pub noise_floor_ns: u64,
    /// Which timing metric to use as the primary regression signal.
    pub primary_metric: PrimaryMetric,
    /// If true, also diff memory (alloc bytes) and flag memory regressions.
    pub check_memory: bool,
    /// Minimum call count for a function to be included in the diff.
    /// Filters out functions called very rarely (high variance).
    pub min_call_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryMetric {
    /// Mean inclusive time. Good for steady-state workloads.
    MeanNs,
    /// P99 inclusive time. Good for latency-sensitive code.
    P99Ns,
    /// P95 inclusive time.
    P95Ns,
    /// Median. Most robust to outliers.
    P50Ns,
    /// Self time (excludes callees).
    SelfNs,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            regression_threshold_pct: 10.0,
            improvement_threshold_pct: 5.0,
            noise_floor_ns: 500,
            primary_metric: PrimaryMetric::MeanNs,
            check_memory: true,
            min_call_count: 3,
        }
    }
}

// ─── severity ─────────────────────────────────────────────────────────────────

/// How severe a regression is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RegressionSeverity {
    /// < 2× threshold. Worth noting, not alarming.
    Minor,
    /// 2–5× threshold. Likely a real regression.
    Moderate,
    /// > 5× threshold. Definite regression — fail CI.
    Critical,
}

// ─── output schema ────────────────────────────────────────────────────────────

/// The full diff between two profiling sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDiff {
    pub baseline_file: String,
    pub current_file: String,
    pub baseline_duration_ns: u64,
    pub current_duration_ns: u64,
    /// Per-function diffs, sorted by absolute change (largest first).
    pub functions: Vec<FunctionDiff>,
    /// Per-benchmark diffs.
    pub benchmarks: Vec<BenchmarkDiff>,
    /// High-level summary counts.
    pub summary: DiffSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub regressions_critical: u32,
    pub regressions_moderate: u32,
    pub regressions_minor: u32,
    pub improvements: u32,
    pub unchanged: u32,
    pub new_functions: u32,
    pub removed_functions: u32,
    pub memory_regressions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDiff {
    pub name: String,
    pub file: String,
    pub module: String,
    pub status: DiffStatus,

    // timing
    pub baseline_mean_ns: Option<f64>,
    pub current_mean_ns: Option<f64>,
    pub mean_change_ns: Option<f64>,
    pub mean_change_pct: Option<f64>,

    pub baseline_p99_ns: Option<u64>,
    pub current_p99_ns: Option<u64>,
    pub p99_change_pct: Option<f64>,

    pub baseline_p95_ns: Option<u64>,
    pub current_p95_ns: Option<u64>,
    pub p95_change_pct: Option<f64>,

    pub baseline_self_ns: Option<u64>,
    pub current_self_ns: Option<u64>,
    pub self_change_pct: Option<f64>,

    // calls
    pub baseline_call_count: Option<u64>,
    pub current_call_count: Option<u64>,

    // memory
    pub baseline_alloc_bytes: Option<u64>,
    pub current_alloc_bytes: Option<u64>,
    pub alloc_change_pct: Option<f64>,

    /// Only set for regressions.
    pub severity: Option<RegressionSeverity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffStatus {
    Regression,
    Improvement,
    Unchanged,
    New,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDiff {
    pub name: String,
    pub status: DiffStatus,
    pub baseline_median_ns: Option<u64>,
    pub current_median_ns: Option<u64>,
    pub median_change_pct: Option<f64>,
    pub baseline_p99_ns: Option<u64>,
    pub current_p99_ns: Option<u64>,
    pub p99_change_pct: Option<f64>,
    pub baseline_throughput: Option<f64>,
    pub current_throughput: Option<f64>,
    pub throughput_change_pct: Option<f64>,
    pub severity: Option<RegressionSeverity>,
}

// ─── implementation ───────────────────────────────────────────────────────────

impl SessionDiff {
    /// Load a `ProfileSession` from a JSON file.
    pub fn load_session(path: &str) -> std::io::Result<ProfileSession> {
        let data = std::fs::read_to_string(path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Compare `baseline` against `current` and return a structured diff.
    pub fn compare(
        baseline: &ProfileSession,
        current: &ProfileSession,
        config: &DiffConfig,
        baseline_path: &str,
        current_path: &str,
    ) -> Self {
        let baseline_map: HashMap<&str, &FunctionRecord> = baseline.functions
            .iter()
            .filter(|f| f.call_count >= config.min_call_count)
            .map(|f| (f.name.as_str(), f))
            .collect();
        let current_map: HashMap<&str, &FunctionRecord> = current.functions
            .iter()
            .filter(|f| f.call_count >= config.min_call_count)
            .map(|f| (f.name.as_str(), f))
            .collect();

        let mut functions: Vec<FunctionDiff> = Vec::new();
        let mut summary = DiffSummary::default();

        // Functions in baseline — check for regression / improvement / removal
        for (name, b) in &baseline_map {
            if let Some(c) = current_map.get(name) {
                let fd = diff_function(b, c, config);
                match fd.status {
                    DiffStatus::Regression => match fd.severity {
                        Some(RegressionSeverity::Critical)  => summary.regressions_critical += 1,
                        Some(RegressionSeverity::Moderate)  => summary.regressions_moderate += 1,
                        _                                   => summary.regressions_minor += 1,
                    },
                    DiffStatus::Improvement => summary.improvements += 1,
                    DiffStatus::Unchanged   => summary.unchanged += 1,
                    _ => {}
                }
                if fd.alloc_change_pct.map_or(false, |p| p > config.regression_threshold_pct) {
                    summary.memory_regressions += 1;
                }
                functions.push(fd);
            } else {
                // Function was removed
                summary.removed_functions += 1;
                functions.push(FunctionDiff {
                    name: name.to_string(),
                    file: b.file.clone(),
                    module: b.module_path.clone(),
                    status: DiffStatus::Removed,
                    baseline_mean_ns: Some(b.timing.mean_ns),
                    current_mean_ns: None,
                    mean_change_ns: None,
                    mean_change_pct: None,
                    baseline_p99_ns: Some(b.timing.p99_ns),
                    current_p99_ns: None,
                    p99_change_pct: None,
                    baseline_p95_ns: Some(b.timing.p95_ns),
                    current_p95_ns: None,
                    p95_change_pct: None,
                    baseline_self_ns: Some(b.timing.self_ns),
                    current_self_ns: None,
                    self_change_pct: None,
                    baseline_call_count: Some(b.call_count),
                    current_call_count: None,
                    baseline_alloc_bytes: b.memory.as_ref().map(|m| m.total_alloc_bytes),
                    current_alloc_bytes: None,
                    alloc_change_pct: None,
                    severity: None,
                });
            }
        }

        // New functions in current
        for (name, c) in &current_map {
            if !baseline_map.contains_key(name) {
                summary.new_functions += 1;
                functions.push(FunctionDiff {
                    name: name.to_string(),
                    file: c.file.clone(),
                    module: c.module_path.clone(),
                    status: DiffStatus::New,
                    baseline_mean_ns: None,
                    current_mean_ns: Some(c.timing.mean_ns),
                    mean_change_ns: None,
                    mean_change_pct: None,
                    baseline_p99_ns: None,
                    current_p99_ns: Some(c.timing.p99_ns),
                    p99_change_pct: None,
                    baseline_p95_ns: None,
                    current_p95_ns: Some(c.timing.p95_ns),
                    p95_change_pct: None,
                    baseline_self_ns: None,
                    current_self_ns: Some(c.timing.self_ns),
                    self_change_pct: None,
                    baseline_call_count: None,
                    current_call_count: Some(c.call_count),
                    baseline_alloc_bytes: None,
                    current_alloc_bytes: c.memory.as_ref().map(|m| m.total_alloc_bytes),
                    alloc_change_pct: None,
                    severity: None,
                });
            }
        }

        // Sort: regressions first (worst first), then improvements, then rest
        functions.sort_by(|a, b| {
            let severity_order = |s: &DiffStatus, sev: &Option<RegressionSeverity>| match s {
                DiffStatus::Regression => sev.map_or(0u8, |s| match s {
                    RegressionSeverity::Critical => 3,
                    RegressionSeverity::Moderate => 2,
                    RegressionSeverity::Minor => 1,
                }),
                DiffStatus::Improvement => 0,
                _ => 0,
            };
            severity_order(&b.status, &b.severity)
                .cmp(&severity_order(&a.status, &a.severity))
        });

        // Benchmarks
        let baseline_bench: HashMap<&str, &BenchmarkRecord> = baseline.benchmarks
            .iter().map(|b| (b.name.as_str(), b)).collect();
        let current_bench: HashMap<&str, &BenchmarkRecord> = current.benchmarks
            .iter().map(|b| (b.name.as_str(), b)).collect();

        let mut benchmarks: Vec<BenchmarkDiff> = Vec::new();
        for (name, b) in &baseline_bench {
            if let Some(c) = current_bench.get(name) {
                benchmarks.push(diff_benchmark(name, b, c, config));
            }
        }
        for (name, c) in &current_bench {
            if !baseline_bench.contains_key(name) {
                benchmarks.push(BenchmarkDiff {
                    name: name.to_string(),
                    status: DiffStatus::New,
                    baseline_median_ns: None,
                    current_median_ns: Some(c.timing.p50_ns),
                    median_change_pct: None,
                    baseline_p99_ns: None,
                    current_p99_ns: Some(c.timing.p99_ns),
                    p99_change_pct: None,
                    baseline_throughput: None,
                    current_throughput: Some(c.throughput_per_sec),
                    throughput_change_pct: None,
                    severity: None,
                });
            }
        }

        SessionDiff {
            baseline_file: baseline_path.to_owned(),
            current_file: current_path.to_owned(),
            baseline_duration_ns: baseline.session_duration_ns,
            current_duration_ns: current.session_duration_ns,
            functions,
            benchmarks,
            summary,
        }
    }

    pub fn has_critical_regressions(&self) -> bool {
        self.summary.regressions_critical > 0
    }

    pub fn has_any_regressions(&self) -> bool {
        self.summary.regressions_critical > 0
            || self.summary.regressions_moderate > 0
            || self.summary.regressions_minor > 0
    }

    pub fn save_json(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Human-readable summary suitable for CI log output.
    pub fn summary_text(&self) -> String {
        let mut lines = vec![
            format!("┌─ RustScope Diff ─────────────────────────────────────────┐"),
            format!("│ Baseline : {}", self.baseline_file),
            format!("│ Current  : {}", self.current_file),
            format!("│ Regressions: {} critical / {} moderate / {} minor",
                self.summary.regressions_critical,
                self.summary.regressions_moderate,
                self.summary.regressions_minor),
            format!("│ Improvements: {}   New: {}   Removed: {}",
                self.summary.improvements,
                self.summary.new_functions,
                self.summary.removed_functions),
            format!("└──────────────────────────────────────────────────────────┘"),
        ];

        for f in self.functions.iter().filter(|f| f.status == DiffStatus::Regression) {
            let pct = f.mean_change_pct.unwrap_or(0.0);
            let sev = f.severity.map_or("minor", |s| match s {
                RegressionSeverity::Critical => "CRITICAL",
                RegressionSeverity::Moderate => "moderate",
                RegressionSeverity::Minor    => "minor",
            });
            lines.push(format!("  ⚠ [{sev}] {} +{:.1}% mean", f.name, pct));
        }
        for f in self.functions.iter().filter(|f| f.status == DiffStatus::Improvement) {
            let pct = f.mean_change_pct.unwrap_or(0.0).abs();
            lines.push(format!("  ✓ [improvement] {} -{:.1}% mean", f.name, pct));
        }

        lines.join("\n")
    }
}

fn diff_function(b: &FunctionRecord, c: &FunctionRecord, config: &DiffConfig) -> FunctionDiff {
    let primary_b = primary_metric_value(b, config.primary_metric);
    let primary_c = primary_metric_value(c, config.primary_metric);
    let change_ns = primary_c as i64 - primary_b as i64;
    let change_pct = if primary_b > 0 {
        change_ns as f64 / primary_b as f64 * 100.0
    } else { 0.0 };

    let alloc_b = b.memory.as_ref().map(|m| m.total_alloc_bytes).unwrap_or(0);
    let alloc_c = c.memory.as_ref().map(|m| m.total_alloc_bytes).unwrap_or(0);
    let alloc_pct = if alloc_b > 0 {
        Some((alloc_c as i64 - alloc_b as i64) as f64 / alloc_b as f64 * 100.0)
    } else { None };

   let status = if (change_ns.abs() as u64) < config.noise_floor_ns {
        DiffStatus::Unchanged
    } else if change_pct > config.regression_threshold_pct {
        DiffStatus::Regression
    } else if change_pct < -(config.improvement_threshold_pct) {
        DiffStatus::Improvement
    } else {
        DiffStatus::Unchanged
    };

    let severity = if status == DiffStatus::Regression {
        let mult = change_pct / config.regression_threshold_pct;
        Some(if mult > 5.0 { RegressionSeverity::Critical }
             else if mult > 2.0 { RegressionSeverity::Moderate }
             else { RegressionSeverity::Minor })
    } else { None };

    FunctionDiff {
        name: b.name.clone(),
        file: b.file.clone(),
        module: b.module_path.clone(),
        status,
        baseline_mean_ns: Some(b.timing.mean_ns),
        current_mean_ns: Some(c.timing.mean_ns),
        mean_change_ns: Some((c.timing.mean_ns - b.timing.mean_ns) as f64),
        mean_change_pct: Some(change_pct),
        baseline_p99_ns: Some(b.timing.p99_ns),
        current_p99_ns: Some(c.timing.p99_ns),
        p99_change_pct: Some(pct(b.timing.p99_ns, c.timing.p99_ns)),
        baseline_p95_ns: Some(b.timing.p95_ns),
        current_p95_ns: Some(c.timing.p95_ns),
        p95_change_pct: Some(pct(b.timing.p95_ns, c.timing.p95_ns)),
        baseline_self_ns: Some(b.timing.self_ns),
        current_self_ns: Some(c.timing.self_ns),
        self_change_pct: Some(pct(b.timing.self_ns, c.timing.self_ns)),
        baseline_call_count: Some(b.call_count),
        current_call_count: Some(c.call_count),
        baseline_alloc_bytes: b.memory.as_ref().map(|m| m.total_alloc_bytes),
        current_alloc_bytes: c.memory.as_ref().map(|m| m.total_alloc_bytes),
        alloc_change_pct: alloc_pct,
        severity,
    }
}

fn diff_benchmark(name: &str, b: &BenchmarkRecord, c: &BenchmarkRecord, config: &DiffConfig) -> BenchmarkDiff {
    let median_pct = pct(b.timing.p50_ns, c.timing.p50_ns);
    let p99_pct = pct(b.timing.p99_ns, c.timing.p99_ns);
    let thru_pct = if b.throughput_per_sec > 0.0 {
        (c.throughput_per_sec - b.throughput_per_sec) / b.throughput_per_sec * 100.0
    } else { 0.0 };

    let status = if median_pct > config.regression_threshold_pct {
        DiffStatus::Regression
    } else if median_pct < -(config.improvement_threshold_pct) {
        DiffStatus::Improvement
    } else {
        DiffStatus::Unchanged
    };

    let severity = if status == DiffStatus::Regression {
        let mult = median_pct / config.regression_threshold_pct;
        Some(if mult > 5.0 { RegressionSeverity::Critical }
             else if mult > 2.0 { RegressionSeverity::Moderate }
             else { RegressionSeverity::Minor })
    } else { None };

    BenchmarkDiff {
        name: name.to_owned(),
        status,
        baseline_median_ns: Some(b.timing.p50_ns),
        current_median_ns: Some(c.timing.p50_ns),
        median_change_pct: Some(median_pct),
        baseline_p99_ns: Some(b.timing.p99_ns),
        current_p99_ns: Some(c.timing.p99_ns),
        p99_change_pct: Some(p99_pct),
        baseline_throughput: Some(b.throughput_per_sec),
        current_throughput: Some(c.throughput_per_sec),
        throughput_change_pct: Some(thru_pct),
        severity,
    }
}

fn primary_metric_value(f: &FunctionRecord, m: PrimaryMetric) -> u64 {
    match m {
        PrimaryMetric::MeanNs => f.timing.mean_ns as u64,
        PrimaryMetric::P99Ns  => f.timing.p99_ns,
        PrimaryMetric::P95Ns  => f.timing.p95_ns,
        PrimaryMetric::P50Ns  => f.timing.p50_ns,
        PrimaryMetric::SelfNs => f.timing.self_ns,
    }
}

fn pct(baseline: u64, current: u64) -> f64 {
    if baseline == 0 { return 0.0; }
    (current as i64 - baseline as i64) as f64 / baseline as f64 * 100.0
}

impl Default for DiffSummary {
    fn default() -> Self {
        Self { regressions_critical: 0, regressions_moderate: 0, regressions_minor: 0,
               improvements: 0, unchanged: 0, new_functions: 0, removed_functions: 0,
               memory_regressions: 0 }
    }
}
