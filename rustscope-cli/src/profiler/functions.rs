use anyhow::Result;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::output::schema::{Function, HotspotSnapshot, Rollup, SamplingDiagnosticsRecord};
use std::fs;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use chrono::Utc;

pub struct FunctionSampler {
    pid: u32,
    functions: Arc<Mutex<Vec<Function>>>,
    snapshots: Arc<Mutex<Vec<HotspotSnapshot>>>,
    diagnostics: Arc<Mutex<SamplingDiagnosticsRecord>>,
}

impl FunctionSampler {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            functions: Arc::new(Mutex::new(Vec::new())),
            snapshots: Arc::new(Mutex::new(Vec::new())),
            diagnostics: Arc::new(Mutex::new(default_diagnostics())),
        }
    }

    #[cfg(target_os = "macos")]
    pub async fn start_sampling(&self, duration_secs: u32) -> Result<()> {
        let max_duration = if duration_secs == 0 { 3600 } else { duration_secs };
        let started = Instant::now();
        let mut aggregate: HashMap<String, u64> = HashMap::new();
        let mut total_samples = 0u64;

        println!("Starting macOS 'sample' for PID {}...", self.pid);
        self.set_backend("macos-sample").await;

        while started.elapsed().as_secs() < max_duration as u64 && process_exists(self.pid) {
            let output_path = format!("/tmp/rustscope_sample_{}_{}.txt", self.pid, started.elapsed().as_millis());
            let status = Command::new("sample")
                .args([
                    &self.pid.to_string(),
                    "1",
                    "5",
                    "-mayDie",
                    "-file",
                    &output_path,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;

            if let Ok(content) = fs::read_to_string(&output_path) {
                let chunk = parse_sample_chunk(&content);
                total_samples += chunk.total_samples;
                self.record_raw_samples(chunk.total_samples).await;
                self.record_snapshot(&chunk.counts, chunk.total_samples).await;
                for (name, count) in &chunk.counts {
                    *aggregate.entry(name.clone()).or_insert(0) += *count;
                }
                let _ = fs::remove_file(&output_path);
            } else if !status.success() && !process_exists(self.pid) {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        if total_samples == 0 {
            eprintln!("Warning: macOS 'sample' did not yield any stack samples.");
            self.mark_fallback_only().await;
            return Ok(());
        }

        self.finalize_symbol_stats(&aggregate).await;

        let mut functions_guard = self.functions.lock().await;
        let mut current_x: f64 = 0.0;
        let mut rows: Vec<_> = aggregate.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));

        functions_guard.clear();
        for (name, count) in rows {
            let pct = (count as f64 / total_samples as f64) * 100.0;
            functions_guard.push(Function {
                module: derive_module(&name),
                name,
                self_pct: (pct * 10.0).round() / 10.0,
                total_pct: (pct * 10.0).round() / 10.0,
                calls: count,
                avg_ns: 0,
                depth: 0,
                x: (current_x * 10.0).round() / 10.0,
                w: (pct * 10.0).round() / 10.0,
            });
            current_x += pct;
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub async fn start_sampling(&self, duration_secs: u32) -> Result<()> {
        use std::path::Path;

        if Command::new("perf").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err() {
            return Ok(());
        }
        self.set_backend("linux-perf").await;

        let duration = if duration_secs == 0 { 30 } else { duration_secs };
        let perf_data = format!("/tmp/rustscope_perf_{}.data", self.pid);
        let perf_script = format!("/tmp/rustscope_perf_{}.script", self.pid);

        let status = Command::new("perf")
            .args([
                "record",
                "-F",
                "99",
                "-g",
                "-p",
                &self.pid.to_string(),
                "-o",
                &perf_data,
                "--",
                "sleep",
                &duration.to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if !status.success() || !Path::new(&perf_data).exists() {
            return Ok(());
        }

        let script_output = Command::new("perf")
            .args(["script", "-i", &perf_data])
            .output()?;

        let _ = fs::remove_file(&perf_data);

        if !script_output.status.success() {
            return Ok(());
        }

        fs::write(&perf_script, &script_output.stdout)?;
        let content = String::from_utf8_lossy(&script_output.stdout);
        let rows = parse_perf_script(&content);
        let _ = fs::remove_file(&perf_script);

        let total_samples: u64 = rows.values().sum();
        if total_samples == 0 {
            self.mark_fallback_only().await;
            return Ok(());
        }
        self.record_raw_samples(total_samples).await;
        self.record_snapshot(&rows, total_samples).await;
        self.finalize_symbol_stats(&rows).await;

        let mut functions_guard = self.functions.lock().await;
        let mut current_x: f64 = 0.0;
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        functions_guard.clear();

        for (name, count) in rows {
            let pct = (count as f64 / total_samples as f64) * 100.0;
            functions_guard.push(Function {
                module: derive_module(&name),
                name,
                self_pct: (pct * 10.0).round() / 10.0,
                total_pct: (pct * 10.0).round() / 10.0,
                calls: count,
                avg_ns: 0,
                depth: 0,
                x: (current_x * 10.0).round() / 10.0,
                w: (pct * 10.0).round() / 10.0,
            });
            current_x += pct;
        }

        Ok(())
    }

    pub async fn get_functions(&self) -> Vec<Function> {
        self.functions.lock().await.clone()
    }

    pub async fn get_snapshots(&self) -> Vec<HotspotSnapshot> {
        self.snapshots.lock().await.clone()
    }

    pub async fn get_diagnostics(&self) -> SamplingDiagnosticsRecord {
        self.diagnostics.lock().await.clone()
    }

    async fn record_snapshot(&self, counts: &HashMap<String, u64>, total_samples: u64) {
        if total_samples == 0 || counts.is_empty() {
            return;
        }

        let mut top_functions: Vec<Rollup> = counts
            .iter()
            .map(|(name, count)| Rollup {
                name: name.clone(),
                total_pct: (*count as f64 / total_samples as f64) * 100.0,
                self_pct: (*count as f64 / total_samples as f64) * 100.0,
                calls: *count,
                function_count: 1,
            })
            .collect();
        top_functions.sort_by(|a, b| b.total_pct.partial_cmp(&a.total_pct).unwrap_or(std::cmp::Ordering::Equal));
        top_functions.truncate(5);

        let crate_rollups = aggregate_rollups(counts, total_samples, RollupLevel::Crate);
        let module_rollups = aggregate_rollups(counts, total_samples, RollupLevel::Module);

        let mut snapshots = self.snapshots.lock().await;
        snapshots.push(HotspotSnapshot {
            ts: Utc::now().timestamp_millis() as u64,
            top_functions,
            crate_rollups,
            module_rollups,
        });
    }

    async fn set_backend(&self, backend: &str) {
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.backend = backend.to_string();
    }

    async fn record_raw_samples(&self, raw_samples: u64) {
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.raw_samples += raw_samples;
    }

    async fn finalize_symbol_stats(&self, counts: &HashMap<String, u64>) {
        let symbolized_samples: u64 = counts.values().sum();
        let unknown_symbols: u64 = counts
            .iter()
            .filter(|(name, _)| is_unknown_symbol(name))
            .map(|(_, count)| *count)
            .sum();

        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.symbolized_samples = symbolized_samples;
        diagnostics.unknown_symbols = unknown_symbols;
        diagnostics.dropped_samples = diagnostics.raw_samples.saturating_sub(symbolized_samples);
        diagnostics.fallback_used = false;

        let coverage = if diagnostics.raw_samples == 0 {
            0.0
        } else {
            diagnostics.symbolized_samples as f64 / diagnostics.raw_samples as f64
        };
        diagnostics.fidelity = if diagnostics.symbolized_samples == 0 {
            "fallback-only".to_string()
        } else if coverage >= 0.90 && unknown_symbols == 0 {
            "high".to_string()
        } else if coverage >= 0.60 {
            "medium".to_string()
        } else {
            "low".to_string()
        };
    }

    async fn mark_fallback_only(&self) {
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.fallback_used = true;
        diagnostics.fidelity = "fallback-only".to_string();
    }
}

fn derive_module(name: &str) -> String {
    let cleaned = name
        .split_whitespace()
        .next()
        .unwrap_or(name)
        .trim_matches(|c| c == '<' || c == '>');

    let parts: Vec<&str> = cleaned.split("::").filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [] => "unknown".to_string(),
        [single] => (*single).to_string(),
        [first, second, ..] => format!("{}::{}", first, second),
    }
}

#[derive(Clone, Copy)]
enum RollupLevel {
    Crate,
    Module,
}

fn aggregate_rollups(counts: &HashMap<String, u64>, total_samples: u64, level: RollupLevel) -> Vec<Rollup> {
    let mut aggregate: HashMap<String, Rollup> = HashMap::new();

    for (name, count) in counts {
        let key = match level {
            RollupLevel::Crate => derive_crate(name),
            RollupLevel::Module => derive_module(name),
        };

        let entry = aggregate.entry(key.clone()).or_insert(Rollup {
            name: key,
            total_pct: 0.0,
            self_pct: 0.0,
            calls: 0,
            function_count: 0,
        });

        let pct = (*count as f64 / total_samples as f64) * 100.0;
        entry.total_pct += pct;
        entry.self_pct += pct;
        entry.calls += *count;
        entry.function_count += 1;
    }

    let mut rollups: Vec<_> = aggregate.into_values().collect();
    rollups.sort_by(|a, b| b.total_pct.partial_cmp(&a.total_pct).unwrap_or(std::cmp::Ordering::Equal));
    rollups.truncate(5);
    rollups
}

fn derive_crate(name: &str) -> String {
    let cleaned = name
        .split_whitespace()
        .next()
        .unwrap_or(name)
        .trim_matches(|c| c == '<' || c == '>');
    cleaned.split("::").next().unwrap_or("unknown").to_string()
}

fn default_diagnostics() -> SamplingDiagnosticsRecord {
    SamplingDiagnosticsRecord {
        backend: platform_backend_name().to_string(),
        raw_samples: 0,
        symbolized_samples: 0,
        dropped_samples: 0,
        unknown_symbols: 0,
        fallback_used: false,
        fidelity: "unknown".to_string(),
    }
}

fn platform_backend_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos-sample"
    }

    #[cfg(target_os = "linux")]
    {
        "linux-perf"
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "unsupported"
    }
}

fn is_unknown_symbol(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("unknown")
        || trimmed.contains("[unknown]")
        || trimmed.contains("???")
}

#[cfg(target_os = "macos")]
struct SampleChunk {
    total_samples: u64,
    counts: HashMap<String, u64>,
}

#[cfg(target_os = "macos")]
fn parse_sample_chunk(content: &str) -> SampleChunk {
    let total_samples = content
        .lines()
        .find_map(|line| {
            if !line.contains("Total number of samples") {
                return None;
            }
            line.split_whitespace().last()?.parse::<u64>().ok()
        })
        .unwrap_or(0);

    let mut counts = HashMap::new();
    let mut in_top_of_stack = false;

    for line in content.lines() {
        if line.contains("Sort by top of stack") {
            in_top_of_stack = true;
            continue;
        }
        if in_top_of_stack && line.trim().is_empty() {
            break;
        }
        if !in_top_of_stack {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        if let Ok(count) = parts[0].parse::<u64>() {
            let symbol = parts[1..].join(" ");
            *counts.entry(symbol).or_insert(0) += count;
        }
    }

    if counts.is_empty() {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            if let Ok(count) = parts[0].parse::<u64>() {
                let symbol = parts[1..].join(" ");
                *counts.entry(symbol).or_insert(0) += count;
            }
        }
    }

    SampleChunk { total_samples, counts }
}

#[cfg(target_os = "macos")]
fn process_exists(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;

    signal::kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(target_os = "linux")]
fn parse_perf_script(content: &str) -> HashMap<String, u64> {
    let mut rows = HashMap::new();
    let mut current_stack: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            if let Some(top) = current_stack.first() {
                *rows.entry(top.clone()).or_insert(0) += 1;
            }
            current_stack.clear();
            continue;
        }

        let trimmed = line.trim();
        if trimmed.starts_with(char::is_numeric) {
            continue;
        }

        let symbol = trimmed
            .split_whitespace()
            .nth(1)
            .unwrap_or(trimmed)
            .to_string();
        current_stack.push(symbol);
    }

    if let Some(top) = current_stack.first() {
        *rows.entry(top.clone()).or_insert(0) += 1;
    }

    rows
}
