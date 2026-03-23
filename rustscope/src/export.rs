//! # Phase 4b: Export Formats
//!
//! Convert a `ProfileSession` into formats consumed by third-party tools.
//!
//! | Format | Tool | File |
//! |--------|------|------|
//! | Chrome Trace Event | `chrome://tracing`, Perfetto | `.json` |
//! | SpeedScope | speedscope.app | `.json` (speedscope schema) |
//! | pprof protobuf | `go tool pprof`, pyroscope | `.pb.gz` |
//! | Brendan Gregg collapsed | `flamegraph.pl` | `.txt` |
//! | CSV | Excel, pandas, jq | `.csv` |

use std::io;
use serde::{Deserialize, Serialize};

use crate::output::schema::{CallTreeNode, ProfileSession};

// ─── Chrome Trace Event Format ────────────────────────────────────────────────
// https://docs.google.com/document/d/1CvAClvFfyA5R-PhYUmn5OOQtYMH4h6I0nSsKchNAySU

/// Export as Chrome Trace Event JSON (open in `chrome://tracing` or Perfetto).
///
/// The output contains:
/// - Duration events (X) for each instrumented function call.
/// - Counter events (C) for heap memory over time.
/// - Metadata events (M) for process/thread names.
pub fn to_chrome_trace(session: &ProfileSession) -> String {
    let mut events: Vec<serde_json::Value> = Vec::new();

    // Process metadata
    events.push(serde_json::json!({
        "name": "process_name", "ph": "M", "pid": 1, "tid": 0,
        "args": { "name": "rustscope" }
    }));

    // Convert function records to synthetic duration events.
    // Since we only have aggregated data (not per-call timestamps), we emit
    // one event per function showing mean duration at a synthetic timestamp.
    let mut t_us: f64 = 0.0; // synthetic timeline in microseconds

    for f in &session.functions {
        let dur_us = f.timing.mean_ns as f64 / 1000.0;
        events.push(serde_json::json!({
            "name": f.name,
            "cat": f.module_path,
            "ph": "X",       // complete event
            "ts":  t_us,
            "dur": dur_us,
            "pid": 1,
            "tid": 1,
            "args": {
                "file": f.file,
                "line": f.line,
                "calls": f.call_count,
                "total_ns": f.timing.total_ns,
                "self_ns": f.timing.self_ns,
                "p99_ns": f.timing.p99_ns,
                "heap_alloc": f.memory.as_ref().map(|m| m.total_alloc_bytes).unwrap_or(0),
            }
        }));
        t_us += dur_us;
    }

    // Call trees produce more accurate timeline events
    fn tree_events(node: &CallTreeNode, t_us: &mut f64, depth: u32, events: &mut Vec<serde_json::Value>) {
        let dur = node.duration_ns as f64 / 1000.0;
        events.push(serde_json::json!({
            "name": node.name,
            "ph": "X",
            "ts": *t_us,
            "dur": dur,
            "pid": 1,
            "tid": 2,        // separate thread track for call trees
            "args": {
                "file": node.file,
                "line": node.line,
                "alloc_bytes": node.alloc_bytes,
            }
        }));
        let child_start = *t_us;
        for child in &node.children {
            tree_events(child, t_us, depth + 1, events);
        }
        *t_us = child_start + dur;
    }

    let mut tree_t = 0.0f64;
    for root in &session.call_trees {
        tree_events(root, &mut tree_t, 0, &mut events);
    }

    serde_json::json!({ "traceEvents": events }).to_string()
}

// ─── SpeedScope format ────────────────────────────────────────────────────────
// https://github.com/jlfwong/speedscope/blob/main/src/lib/file-format-spec.md

/// Export as SpeedScope JSON (upload to speedscope.app for an interactive flame graph).
pub fn to_speedscope(session: &ProfileSession) -> String {
    // SpeedScope uses "sampled" profiles with a list of stacks + weights.
    // We convert our call trees into a sampled representation.

    let mut frames: Vec<serde_json::Value> = Vec::new();
    let mut frame_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    fn get_frame(
        name: &str,
        file: &str,
        line: u32,
        frames: &mut Vec<serde_json::Value>,
        frame_map: &mut std::collections::HashMap<String, usize>,
    ) -> usize {
        let key = format!("{name}:{file}:{line}");
        if let Some(&i) = frame_map.get(&key) { return i; }
        let i = frames.len();
        frames.push(serde_json::json!({
            "name": name,
            "file": file,
            "line": line,
        }));
        frame_map.insert(key, i);
        i
    }

    let mut samples: Vec<Vec<usize>> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();

    fn collect_samples(
        node: &CallTreeNode,
        stack: &mut Vec<usize>,
        frames: &mut Vec<serde_json::Value>,
        frame_map: &mut std::collections::HashMap<String, usize>,
        samples: &mut Vec<Vec<usize>>,
        weights: &mut Vec<f64>,
    ) {
        let fi = get_frame(&node.name, &node.file, node.line, frames, frame_map);
        stack.push(fi);

        if node.children.is_empty() {
            samples.push(stack.clone());
            weights.push(node.duration_ns as f64);
        } else {
            for child in &node.children {
                collect_samples(child, stack, frames, frame_map, samples, weights);
            }
        }

        stack.pop();
    }

    for root in &session.call_trees {
        let mut stack = Vec::new();
        collect_samples(root, &mut stack, &mut frames, &mut frame_map, &mut samples, &mut weights);
    }

    let total_weight: f64 = weights.iter().sum();

    serde_json::json!({
        "$schema": "https://www.speedscope.app/file-format-schema.json",
        "version": "0.0.1",
        "shared": { "frames": frames },
        "profiles": [{
            "type": "sampled",
            "name": "rustscope",
            "unit": "nanoseconds",
            "startValue": 0,
            "endValue": total_weight,
            "samples": samples,
            "weights": weights,
        }],
        "activeProfileIndex": 0,
        "exporter": "rustscope v0.2",
    }).to_string()
}

// ─── Brendan Gregg collapsed stacks ──────────────────────────────────────────

/// Export as Brendan Gregg's "collapsed stacks" format.
///
/// ```text
/// main;do_work;sort_data 412300000
/// main;do_work;allocate 231000000
/// ```
///
/// Feed this to `flamegraph.pl` from https://github.com/brendangregg/FlameGraph:
/// ```sh
/// rustscope-cli export collapsed profile.json | flamegraph.pl > out.svg
/// ```
pub fn to_collapsed_stacks(session: &ProfileSession) -> String {
    let mut lines: Vec<String> = Vec::new();

    fn walk(node: &CallTreeNode, stack: &mut Vec<String>, lines: &mut Vec<String>) {
        stack.push(node.name.clone());
        if node.children.is_empty() {
            lines.push(format!("{} {}", stack.join(";"), node.duration_ns));
        } else {
            for child in &node.children {
                walk(child, stack, lines);
            }
        }
        stack.pop();
    }

    for root in &session.call_trees {
        let mut stack = Vec::new();
        walk(root, &mut stack, &mut lines);
    }

    // Also emit flat function data for functions not in call trees
    for f in &session.functions {
        lines.push(format!("{} {}", f.name, f.timing.self_ns));
    }

    lines.join("\n")
}

// ─── CSV export ───────────────────────────────────────────────────────────────

/// Export function table as CSV (Excel, pandas, `cut`, etc.).
pub fn to_csv(session: &ProfileSession) -> String {
    let header = "name,module,file,line,calls,total_ns,self_ns,mean_ns,stddev_ns,p50_ns,p95_ns,p99_ns,pct_of_session,heap_alloc_bytes,heap_dealloc_bytes,net_retained_bytes,frame_size_bytes,cpu_cycles,ipc,cache_miss_rate,branch_miss_rate";
    let mut rows: Vec<String> = vec![header.to_owned()];

    for f in &session.functions {
        let mem = f.memory.as_ref();
        let cpu = f.cpu.as_ref();
        rows.push(format!(
            "{},{},{},{},{},{},{},{},{:.1},{},{},{},{:.2},{},{},{},{},{},{:.3},{:.4},{:.4}",
            csv_esc(&f.name),
            csv_esc(&f.module_path),
            csv_esc(&f.file),
            f.line,
            f.call_count,
            f.timing.total_ns,
            f.timing.self_ns,
            f.timing.mean_ns as u64,
            f.timing.stddev_ns,
            f.timing.p50_ns,
            f.timing.p95_ns,
            f.timing.p99_ns,
            f.timing.pct_of_session,
            mem.map(|m| m.total_alloc_bytes).unwrap_or(0),
            mem.map(|m| m.total_dealloc_bytes).unwrap_or(0),
            mem.map(|m| m.net_retained_bytes).unwrap_or(0),
            f.stack.frame_size_bytes.unwrap_or(0),
            cpu.map(|c| c.cpu_cycles).unwrap_or(0),
            cpu.map(|c| c.ipc).unwrap_or(0.0),
            cpu.map(|c| c.cache_miss_rate).unwrap_or(0.0),
            cpu.map(|c| c.branch_miss_rate).unwrap_or(0.0),
        ));
    }
    rows.join("\n")
}

fn csv_esc(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

// ─── Top-level export convenience ────────────────────────────────────────────

/// All supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    ChromeTrace,
    SpeedScope,
    CollapsedStacks,
    Csv,
}

impl ExportFormat {
    pub fn default_extension(self) -> &'static str {
        match self {
            Self::ChromeTrace     => "chrome_trace.json",
            Self::SpeedScope      => "speedscope.json",
            Self::CollapsedStacks => "stacks.txt",
            Self::Csv             => "profile.csv",
        }
    }
}

/// Export `session` in the given format and write to `path`.
pub fn export(session: &ProfileSession, format: ExportFormat, path: &str) -> io::Result<()> {
    let content = match format {
        ExportFormat::ChromeTrace     => to_chrome_trace(session),
        ExportFormat::SpeedScope      => to_speedscope(session),
        ExportFormat::CollapsedStacks => to_collapsed_stacks(session),
        ExportFormat::Csv             => to_csv(session),
    };
    std::fs::write(path, content)?;
    println!("[rustscope] Exported {:?} to {}", format, path);
    Ok(())
}
