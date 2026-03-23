//! # Outlier detection and latency budgets
//!
//! ## Outlier detection
//!
//! Uses Welford's online algorithm to maintain a running mean and standard
//! deviation per function. Any call more than `threshold_sigma` (default 3σ)
//! above the mean is flagged as an outlier in the timeline and JSON.
//!
//! ```rust
//! // Outliers are automatically flagged in timeline events and function records.
//! // You can also query them:
//! let outliers = rustscope::outliers::get_outliers();
//! for o in &outliers {
//!     println!("{}: call #{} took {}ns ({}σ above mean)",
//!         o.function, o.call_index, o.duration_ns, o.sigma);
//! }
//! rustscope::outliers::save_json("outliers.json").unwrap();
//! ```
//!
//! ## Latency budgets
//!
//! Register a budget per function. Any call exceeding it fires a callback
//! and is flagged in the timeline.
//!
//! ```rust
//! // Register a 5ms budget for `my_handler`
//! rustscope::outliers::set_budget("my_handler", 5_000_000);
//!
//! // Or with a custom callback:
//! rustscope::outliers::set_budget_with_callback("my_handler", 5_000_000, |info| {
//!     eprintln!("BUDGET EXCEEDED: {} took {}ns", info.function, info.duration_ns);
//!     // Could also write to a metrics system, send an alert, etc.
//! });
//! ```

use std::collections::HashMap;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::AtomicBool;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// ─── config ───────────────────────────────────────────────────────────────────

/// Standard deviations above mean to classify a call as an outlier.
static OUTLIER_SIGMA: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(30); // stored as sigma * 10
static OUTLIER_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_outlier_threshold_sigma(sigma: f64) {
    OUTLIER_SIGMA.store((sigma * 10.0) as u32, Relaxed);
}
pub fn outlier_threshold_sigma() -> f64 {
    OUTLIER_SIGMA.load(Relaxed) as f64 / 10.0
}
pub fn disable_outlier_detection() { OUTLIER_ENABLED.store(false, Relaxed); }
pub fn enable_outlier_detection()  { OUTLIER_ENABLED.store(true, Relaxed); }

// ─── per-function running stats for outlier detection ─────────────────────────

#[derive(Default)]
struct RunningStats {
    count: u64,
    mean: f64,
    m2: f64,   // Welford's M2 for variance
}

impl RunningStats {
    fn update(&mut self, x: f64) {
        self.count += 1;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (x - self.mean);
    }

    fn stddev(&self) -> f64 {
        if self.count < 2 { return f64::INFINITY; }
        (self.m2 / (self.count - 1) as f64).sqrt()
    }

    fn is_outlier(&self, x: f64, threshold_sigma: f64) -> (bool, f64) {
        if self.count < 10 {
            // Not enough data to establish a baseline — never flag as outlier
            return (false, 0.0);
        }
        let sd = self.stddev();
        if sd == 0.0 { return (false, 0.0); }
        let sigma = (x - self.mean) / sd;
        (sigma > threshold_sigma, sigma)
    }
}

// ─── outlier record ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlierRecord {
    pub function: String,
    /// Sequential call index for this function (1-based).
    pub call_index: u64,
    /// Duration of this outlier call (ns).
    pub duration_ns: u64,
    /// How many standard deviations above the mean.
    pub sigma: f64,
    /// Mean duration at the time of this call (ns).
    pub baseline_mean_ns: f64,
    /// Stddev at time of this call (ns).
    pub baseline_stddev_ns: f64,
    /// Thread number where the outlier occurred.
    pub thread: u64,
    /// Nanoseconds since session start.
    pub timestamp_ns: u64,
}

// ─── budget record ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetViolation {
    pub function: String,
    pub call_index: u64,
    pub duration_ns: u64,
    pub budget_ns: u64,
    /// How much over budget (ns).
    pub exceeded_by_ns: u64,
    /// Percentage over budget.
    pub exceeded_pct: f64,
    pub thread: u64,
    pub timestamp_ns: u64,
}

// ─── global state ─────────────────────────────────────────────────────────────

type BudgetCallback = Box<dyn Fn(&BudgetViolation) + Send + Sync>;

struct GlobalOutliers {
    stats: HashMap<String, RunningStats>,
    outliers: Vec<OutlierRecord>,
    budgets: HashMap<String, u64>,              // fn_name → budget_ns
    budget_callbacks: HashMap<String, BudgetCallback>,
    violations: Vec<BudgetViolation>,
}

impl GlobalOutliers {
    fn new() -> Self {
        Self {
            stats: HashMap::new(),
            outliers: Vec::new(),
            budgets: HashMap::new(),
            budget_callbacks: HashMap::new(),
            violations: Vec::new(),
        }
    }
}

static STATE: Lazy<Mutex<GlobalOutliers>> = Lazy::new(|| Mutex::new(GlobalOutliers::new()));

// ─── public API ──────────────────────────────────────────────────────────────

/// Register a latency budget for a function. Any call exceeding `budget_ns`
/// is recorded in `BudgetViolation` records and flagged in the timeline.
pub fn set_budget(fn_name: &str, budget_ns: u64) {
    STATE.lock().budgets.insert(fn_name.to_owned(), budget_ns);
}

/// Register a budget with a custom callback fired on every violation.
pub fn set_budget_with_callback<F>(fn_name: &str, budget_ns: u64, callback: F)
where
    F: Fn(&BudgetViolation) + Send + Sync + 'static,
{
    let mut s = STATE.lock();
    s.budgets.insert(fn_name.to_owned(), budget_ns);
    s.budget_callbacks.insert(fn_name.to_owned(), Box::new(callback));
}

/// Remove a budget.
pub fn remove_budget(fn_name: &str) {
    let mut s = STATE.lock();
    s.budgets.remove(fn_name);
    s.budget_callbacks.remove(fn_name);
}

/// Called by ProfileGuard::drop() on every function exit.
/// Returns `(is_outlier, budget_exceeded)`.
#[inline]
pub(crate) fn check(
    fn_name: &str,
    duration_ns: u64,
    call_index: u64,
    thread: u64,
    timestamp_ns: u64,
) -> (bool, bool) {
    let outlier_on = OUTLIER_ENABLED.load(Relaxed);

    // Fast path: if outlier detection off AND no budget for this fn, skip entirely
    if !outlier_on {
        let has_budget = STATE.lock().budgets.contains_key(fn_name);
        if !has_budget { return (false, false); }
    }

    let mut is_outlier = false;
    let mut budget_exceeded = false;
    let sigma_threshold = outlier_threshold_sigma();
    let mut s = STATE.lock();

    // Outlier detection
    if outlier_on {
        let stats = s.stats.entry(fn_name.to_owned()).or_default();
        let x = duration_ns as f64;
        let (flagged, sigma) = stats.is_outlier(x, sigma_threshold);
        let mean_ns = stats.mean;
        let stddev_ns = stats.stddev();
        stats.update(x);

        if flagged {
            is_outlier = true;
            s.outliers.push(OutlierRecord {
                function: fn_name.to_owned(),
                call_index,
                duration_ns,
                sigma,
                baseline_mean_ns: mean_ns,
                baseline_stddev_ns: stddev_ns,
                thread,
                timestamp_ns,
            });
        }
    } else {
        s.stats.entry(fn_name.to_owned()).or_default().update(duration_ns as f64);
    }

    // Budget check
    if let Some(&budget_ns) = s.budgets.get(fn_name) {
        if duration_ns > budget_ns {
            budget_exceeded = true;
            let exceeded_by = duration_ns - budget_ns;
            let exceeded_pct = exceeded_by as f64 / budget_ns as f64 * 100.0;
            let violation = BudgetViolation {
                function: fn_name.to_owned(),
                call_index,
                duration_ns,
                budget_ns,
                exceeded_by_ns: exceeded_by,
                exceeded_pct,
                thread,
                timestamp_ns,
            };
            // Fire callback before pushing (callback may need to log quickly)
            if let Some(cb) = s.budget_callbacks.get(fn_name) {
                cb(&violation);
            }
            s.violations.push(violation);
        }
    }

    (is_outlier, budget_exceeded)
}

/// All outlier records collected so far.
pub fn get_outliers() -> Vec<OutlierRecord> {
    STATE.lock().outliers.clone()
}

/// All budget violation records.
pub fn get_violations() -> Vec<BudgetViolation> {
    STATE.lock().violations.clone()
}

/// Save outlier records to JSON.
pub fn save_outliers_json(path: &str) -> std::io::Result<()> {
    let records = get_outliers();
    let json = serde_json::to_string_pretty(&records)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)?;
    println!("[rustscope/outliers] {} outliers written to {}", records.len(), path);
    Ok(())
}

/// Save budget violations to JSON.
pub fn save_violations_json(path: &str) -> std::io::Result<()> {
    let records = get_violations();
    let json = serde_json::to_string_pretty(&records)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)?;
    println!("[rustscope/outliers] {} violations written to {}", records.len(), path);
    Ok(())
}

/// Reset all outlier and violation state (does not reset budgets).
pub fn reset() {
    let mut s = STATE.lock();
    s.stats.clear();
    s.outliers.clear();
    s.violations.clear();
}

/// Outlier + violation summary attached to ProfileSession JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutlierSummary {
    pub total_outliers: usize,
    pub total_budget_violations: usize,
    /// Top 10 outliers by sigma.
    pub top_outliers: Vec<OutlierRecord>,
    /// Top 10 budget violations by exceeded_pct.
    pub top_violations: Vec<BudgetViolation>,
}

pub fn build_summary() -> OutlierSummary {
    let s = STATE.lock();
    let mut outliers = s.outliers.clone();
    let mut violations = s.violations.clone();
    outliers.sort_by(|a, b| b.sigma.partial_cmp(&a.sigma).unwrap_or(std::cmp::Ordering::Equal));
    violations.sort_by(|a, b| b.exceeded_pct.partial_cmp(&a.exceeded_pct).unwrap_or(std::cmp::Ordering::Equal));
    OutlierSummary {
        total_outliers: outliers.len(),
        total_budget_violations: violations.len(),
        top_outliers: outliers.into_iter().take(10).collect(),
        top_violations: violations.into_iter().take(10).collect(),
    }
}
