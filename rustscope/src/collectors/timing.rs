//! Nanosecond timing utilities and percentile computation.

use std::time::Instant;
use crate::output::schema::TimingStats;

/// A running accumulator for timing samples.
/// Uses Welford's online algorithm for variance (no need to store all samples).
#[derive(Default, Clone)]
pub struct TimingAccumulator {
    pub count: u64,
    pub total_ns: u64,
    pub self_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    /// Welford M2 for variance (sum of squared deviations from running mean)
    m2: f64,
    pub mean: f64,
    /// Reservoir of up to `MAX_RESERVOIR` samples for percentile computation.
    reservoir: Vec<u64>,
}

const MAX_RESERVOIR: usize = 10_000;

impl TimingAccumulator {
    pub fn new() -> Self {
        Self { min_ns: u64::MAX, ..Default::default() }
    }

    /// Record one observation (both inclusive and self-time).
    pub fn record(&mut self, total_ns: u64, self_ns: u64) {
        self.count += 1;
        self.total_ns += total_ns;
        self.self_ns += self_ns;
        if total_ns < self.min_ns { self.min_ns = total_ns; }
        if total_ns > self.max_ns { self.max_ns = total_ns; }

        // Welford online variance update
        let delta = total_ns as f64 - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = total_ns as f64 - self.mean;
        self.m2 += delta * delta2;

        // Reservoir sampling for percentiles
        if self.reservoir.len() < MAX_RESERVOIR {
            self.reservoir.push(total_ns);
        } else {
            // Replace a random element (simple linear-congruential pick)
            let idx = (self.count as usize).wrapping_mul(6364136223) % MAX_RESERVOIR;
            self.reservoir[idx] = total_ns;
        }
    }

    pub fn population_stddev(&self) -> f64 {
        if self.count < 2 { 0.0 }
        else { (self.m2 / self.count as f64).sqrt() }
    }

    /// Compute percentiles from the reservoir. Returns (p50, p95, p99).
    pub fn percentiles(&self) -> (u64, u64, u64) {
        if self.reservoir.is_empty() { return (0, 0, 0); }
        let mut sorted = self.reservoir.clone();
        sorted.sort_unstable();
        let p = |pct: f64| -> u64 {
            let idx = ((sorted.len() - 1) as f64 * pct).round() as usize;
            sorted[idx.min(sorted.len() - 1)]
        };
        (p(0.50), p(0.95), p(0.99))
    }

    pub fn build_stats(&self, session_total_ns: u64) -> TimingStats {
        let (p50, p95, p99) = self.percentiles();
        let pct = if session_total_ns > 0 {
            self.total_ns as f64 / session_total_ns as f64 * 100.0
        } else { 0.0 };
        TimingStats {
            total_ns: self.total_ns,
            self_ns: self.self_ns,
            avg_ns: if self.count > 0 { self.total_ns / self.count } else { 0 },
            min_ns: if self.min_ns == u64::MAX { 0 } else { self.min_ns },
            max_ns: self.max_ns,
            mean_ns: self.mean,
            stddev_ns: self.population_stddev(),
            p50_ns: p50,
            p95_ns: p95,
            p99_ns: p99,
            pct_of_session: pct,
        }
    }
}

/// Convenience: measure how long `f()` takes, returns (result, elapsed_ns).
#[inline]
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let t0 = Instant::now();
    let v = f();
    let ns = t0.elapsed().as_nanos() as u64;
    (v, ns)
}
