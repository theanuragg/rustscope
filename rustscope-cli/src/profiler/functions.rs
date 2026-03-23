use anyhow::Result;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::output::schema::Function;
use std::fs;

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
        // Use the built-in 'sample' command on macOS
        // sample <pid> <duration> -f <output_path>
        let output_path = format!("/tmp/rustscope_sample_{}.txt", self.pid);
        let duration = if duration_secs == 0 { 3600 } else { duration_secs };
        
        let mut child = Command::new("sample")
            .args([&self.pid.to_string(), &duration.to_string(), "-f", &output_path])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        // Wait for the process to finish or be killed
        let _ = child.wait();

        // Parse the output if it exists
        if let Ok(content) = fs::read_to_string(&output_path) {
            self.parse_sample_output(&content)?;
            let _ = fs::remove_file(&output_path);
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn parse_sample_output(&self, content: &str) -> Result<()> {
        // Simple parser for macOS 'sample' output
        // It's a complex format, but we can extract function names and counts
        let mut functions_guard = self.functions.try_lock().map_err(|_| anyhow::anyhow!("Failed to lock functions"))?;
        let mut current_total_samples = 0;
        
        // Find the "Total number of samples" line
        for line in content.lines() {
            if line.contains("Total number of samples") {
                if let Some(count_str) = line.split_whitespace().last() {
                    current_total_samples = count_str.parse().unwrap_or(0);
                }
                break;
            }
        }

        if current_total_samples == 0 {
            return Ok(());
        }

        // Look for the call graph section or the "Sort by top of stack" section
        let mut in_top_of_stack = false;
        for line in content.lines() {
            if line.contains("Sort by top of stack") {
                in_top_of_stack = true;
                continue;
            }
            if in_top_of_stack && line.trim().is_empty() {
                break;
            }

            if in_top_of_stack {
                // Line format: <count> <name> (<module>)
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(count) = parts[0].parse::<u64>() {
                        let name = parts[1..].join(" ");
                        let self_pct = (count as f64 / current_total_samples as f64) * 100.0;
                        functions_guard.push(Function {
                            name,
                            module: "unknown".to_string(),
                            self_pct: (self_pct * 10.0).round() / 10.0,
                            total_pct: (self_pct * 10.0).round() / 10.0, // simplified
                            calls: count,
                            avg_ns: 0,
                        });
                    }
                }
            }
        }

        // Sort by self_pct descending as per rules
        functions_guard.sort_by(|a, b| b.self_pct.partial_cmp(&a.self_pct).unwrap());

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
