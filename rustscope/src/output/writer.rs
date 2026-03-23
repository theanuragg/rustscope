//! Writes a `ProfileSession` to disk as JSON.
//!
//! Supports:
//! - Pretty-printed JSON (default, human-readable)
//! - Compact JSON (for CI / log ingestion)
//! - NDJSON append mode (one record per line, for long-running processes)

use std::io::{self, Write};
use std::fs::{File, OpenOptions};
use std::path::Path;

use super::schema::ProfileSession;

/// Options controlling how JSON is written.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// Pretty-print with indentation (default: true).
    pub pretty: bool,
    /// Open file in append mode — writes a newline-delimited JSON record.
    /// Useful for long-running processes or CI runs.
    pub append: bool,
    /// Include raw per-iteration timing samples in benchmark records.
    /// Can produce large files. Default: true (capped at 10 000 samples).
    pub include_raw_samples: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self { pretty: true, append: false, include_raw_samples: true }
    }
}

/// Write `session` to `path` as JSON.
pub fn write_json(path: &Path, session: &ProfileSession, opts: &WriteOptions) -> io::Result<()> {
    let json = if opts.pretty {
        serde_json::to_string_pretty(session)
    } else {
        serde_json::to_string(session)
    }
    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    if opts.append {
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{}", json)?;
    } else {
        std::fs::write(path, &json)?;
    }

    Ok(())
}

/// Write a minimal human-readable summary to stdout (not JSON).
pub fn print_summary(session: &ProfileSession) {
    let total_ms = session.session_duration_ns as f64 / 1_000_000.0;

    println!();
    println!("┌─ RustScope ───────────────────────────────────────────────────────┐");
    println!("│ Session duration : {:.2}ms", total_ms);
    println!("│ Functions tracked: {}", session.functions.len());
    println!("│ Benchmarks run   : {}", session.benchmarks.len());
    if session.session_memory.peak_heap_bytes > 0 {
        println!("│ Peak heap        : {}", fmt_bytes(session.session_memory.peak_heap_bytes));
    }
    // Show metadata if attached
    if let Some(meta) = &session.session_meta {
        if let Some(name) = meta.get("session_name").and_then(|v| v.as_str()) {
            println!("│ Session name     : {}", name);
        }
        if let Some(kv) = meta.get("kv").and_then(|v| v.as_object()) {
            if let Some(commit) = kv.get("git_commit").and_then(|v| v.as_str()) {
                println!("│ Git commit       : {}", commit);
            }
        }
        if let Some(tags) = meta.get("tags").and_then(|v| v.as_array()) {
            if !tags.is_empty() {
                let tag_str: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
                println!("│ Tags             : {}", tag_str.join(", "));
            }
        }
    }
    // Show outlier/budget summary
    if let Some(os) = &session.outlier_summary {
        let n_out = os.get("total_outliers").and_then(|v| v.as_u64()).unwrap_or(0);
        let n_vio = os.get("total_budget_violations").and_then(|v| v.as_u64()).unwrap_or(0);
        if n_out > 0 || n_vio > 0 {
            println!("│ Outliers / SLO   : {} outliers  {} budget violations", n_out, n_vio);
        }
    }
    // Show timeline summary
    if let Some(ts) = &session.timeline_summary {
        let n_ev = ts.get("total_events").and_then(|v| v.as_u64()).unwrap_or(0);
        if n_ev > 0 {
            println!("│ Timeline events  : {}", n_ev);
        }
    }
    println!("└───────────────────────────────────────────────────────────────────┘");
    println!();

    if session.functions.is_empty() {
        println!("  (no profiled functions — add #[profile] or profile_scope!())");
        return;
    }

    // Header
    println!(
        "{:<42} {:>7} {:>12} {:>12} {:>10} {:>10}",
        "Function", "Calls", "TotalTime", "SelfTime", "Avg", "Heap"
    );
    println!("{}", "─".repeat(96));

    let mut fns = session.functions.clone();
    fns.sort_by(|a, b| b.timing.total_ns.cmp(&a.timing.total_ns));

    for f in fns.iter().take(30) {
        let mem_str = f.memory.as_ref()
            .map(|m| fmt_bytes(m.total_alloc_bytes))
            .unwrap_or_else(|| "—".into());

        println!(
            "{:<42} {:>7} {:>11} {:>11} {:>9} {:>10}",
            truncate(&f.name, 42),
            f.call_count,
            fmt_ns(f.timing.total_ns),
            fmt_ns(f.timing.self_ns),
            fmt_ns(f.timing.mean_ns as u64),
            mem_str,
        );

        // Show CPU counters inline if available
        if let Some(cpu) = &f.cpu {
            println!(
                "   └─ cpu cycles={} insns={} IPC={:.2} cache-miss={:.1}% branch-miss={:.1}%",
                fmt_large(cpu.cpu_cycles),
                fmt_large(cpu.instructions),
                cpu.ipc,
                cpu.cache_miss_rate * 100.0,
                cpu.branch_miss_rate * 100.0,
            );
        }
        if let Some(s) = &f.stack.frame_size_bytes {
            if *s > 0 {
                println!("   └─ stack frame ≈ {}", fmt_bytes(*s));
            }
        }
    }
    println!();

    // Benchmarks
    if !session.benchmarks.is_empty() {
        println!("Benchmarks:");
        println!("{:<42} {:>8} {:>12} {:>12} {:>12} {:>14}", "Name", "Iters", "Median", "P95", "P99", "Throughput/s");
        println!("{}", "─".repeat(104));
        for b in &session.benchmarks {
            println!(
                "{:<42} {:>8} {:>12} {:>12} {:>12} {:>14}",
                truncate(&b.name, 42),
                b.iterations,
                fmt_ns(b.timing.p50_ns),
                fmt_ns(b.timing.p95_ns),
                fmt_ns(b.timing.p99_ns),
                format!("{:.0}", b.throughput_per_sec),
            );
        }
        println!();
    }
}

// ── formatting helpers ────────────────────────────────────────────────────────

pub fn fmt_ns(ns: u64) -> String {
    if ns == 0 { return "0ns".into(); }
    if ns < 1_000 { return format!("{}ns", ns); }
    if ns < 1_000_000 { return format!("{:.2}µs", ns as f64 / 1_000.0); }
    if ns < 1_000_000_000 { return format!("{:.2}ms", ns as f64 / 1_000_000.0); }
    format!("{:.3}s", ns as f64 / 1_000_000_000.0)
}

pub fn fmt_bytes(b: u64) -> String {
    if b == 0 { return "0B".into(); }
    if b < 1024 { return format!("{}B", b); }
    if b < 1024 * 1024 { return format!("{:.1}KB", b as f64 / 1024.0); }
    if b < 1024 * 1024 * 1024 { return format!("{:.1}MB", b as f64 / (1024.0 * 1024.0)); }
    format!("{:.1}GB", b as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn fmt_large(n: u64) -> String {
    if n >= 1_000_000_000 { return format!("{:.1}G", n as f64 / 1e9); }
    if n >= 1_000_000 { return format!("{:.1}M", n as f64 / 1e6); }
    if n >= 1_000 { return format!("{:.1}K", n as f64 / 1e3); }
    format!("{}", n)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_owned() }
    else { format!("{}…", &s[..max - 1]) }
}
