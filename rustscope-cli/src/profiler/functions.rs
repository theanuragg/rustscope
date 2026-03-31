use anyhow::Result;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::output::schema::Function;
use std::fs;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct FunctionSampler {
    pid: u32,
    functions: Arc<Mutex<Vec<Function>>>,
}

impl FunctionSampler {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            functions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[cfg(target_os = "macos")]
    pub async fn start_sampling(&self, duration_secs: u32) -> Result<()> {
        let max_duration = if duration_secs == 0 { 3600 } else { duration_secs };
        let started = Instant::now();
        let mut aggregate: HashMap<String, u64> = HashMap::new();
        let mut total_samples = 0u64;

        println!("Starting macOS 'sample' for PID {}...", self.pid);

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
                for (name, count) in chunk.counts {
                    *aggregate.entry(name).or_insert(0) += count;
                }
                let _ = fs::remove_file(&output_path);
            } else if !status.success() && !process_exists(self.pid) {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        if total_samples == 0 {
            eprintln!("Warning: macOS 'sample' did not yield any stack samples.");
            return Ok(());
        }

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
    pub async fn start_sampling(&self, _duration_secs: u32) -> Result<()> {
        // TODO: Implement perf record sidecar for Linux
        Ok(())
    }

    pub async fn get_functions(&self) -> Vec<Function> {
        self.functions.lock().await.clone()
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
