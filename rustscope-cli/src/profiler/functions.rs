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
        
        println!("Starting macOS 'sample' for PID {}...", self.pid);
        let mut child = Command::new("sample")
            .args([&self.pid.to_string(), &duration.to_string(), "-f", &output_path])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        // Wait for the process to finish or be killed
        let status = child.wait()?;
        if !status.success() {
            eprintln!("Warning: macOS 'sample' exited with error. Function data might be missing.");
        }

        // Parse the output if it exists
        if let Ok(content) = fs::read_to_string(&output_path) {
            println!("Parsing 'sample' output ({} bytes)...", content.len());
            self.parse_sample_output(&content)?;
            let _ = fs::remove_file(&output_path);
        } else {
            eprintln!("Warning: Could not read 'sample' output at {}", output_path);
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn parse_sample_output(&self, content: &str) -> Result<()> {
        let mut functions_guard = self.functions.try_lock().map_err(|_| anyhow::anyhow!("Failed to lock functions"))?;
        
        let mut total_samples = 0;
        for line in content.lines() {
            if line.contains("Total number of samples") {
                if let Some(count_str) = line.split_whitespace().last() {
                    total_samples = count_str.parse().unwrap_or(0);
                }
                break;
            }
        }

        if total_samples == 0 {
            return Ok(());
        }

        // Parse Call Graph for hierarchy
        let mut in_call_graph = false;
        let mut current_depth_stack: Vec<(u32, f64, f64)> = Vec::new(); // (indent, x, w)
        let mut last_indent = 0;
        let mut current_x: f64 = 0.0;

        for line in content.lines() {
            if line.contains("Call graph:") {
                in_call_graph = true;
                continue;
            }
            if in_call_graph && (line.trim().is_empty() || line.contains("Binary Images:")) {
                break;
            }

            if in_call_graph {
                let indent = line.chars().take_while(|c| c.is_whitespace()).count() as u32;
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                // Line format: <count> <name>...
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() < 2 { continue; }

                if let Ok(count) = parts[0].parse::<u64>() {
                    let name = parts[1..].join(" ");
                    let total_pct = (count as f64 / total_samples as f64) * 100.0;
                    
                    if indent > last_indent {
                        // Moving deeper
                        current_depth_stack.push((last_indent, current_x, total_pct));
                    } else if indent < last_indent {
                        // Moving shallower
                        while let Some((prev_indent, _, _)) = current_depth_stack.last() {
                            if *prev_indent >= indent {
                                current_depth_stack.pop();
                            } else {
                                break;
                            }
                        }
                    }

                    let depth = current_depth_stack.len() as u32;
                    // For x, we'd need more complex state to track sibling positions.
                    // Simplified: use current_x and increment it for roots
                    let x = if depth == 0 {
                        let val = current_x;
                        current_x += total_pct;
                        val
                    } else {
                        current_depth_stack.last().map(|s| s.1).unwrap_or(0.0)
                    };

                    functions_guard.push(Function {
                        name,
                        module: "unknown".to_string(),
                        self_pct: (total_pct * 10.0).round() / 10.0,
                        total_pct: (total_pct * 10.0).round() / 10.0,
                        calls: count,
                        avg_ns: 0,
                        depth,
                        x: (x * 10.0).round() / 10.0,
                        w: (total_pct * 10.0).round() / 10.0,
                    });

                    last_indent = indent;
                }
            }
        }

        if functions_guard.is_empty() {
            // Fallback to "top of stack" if call graph parsing failed or was empty
            self.parse_top_of_stack(content, &mut functions_guard, total_samples)?;
        }

        Ok(())
    }

    fn parse_top_of_stack(&self, content: &str, functions_guard: &mut Vec<Function>, total_samples: u64) -> Result<()> {
        let mut in_top_of_stack = false;
        let mut current_x: f64 = 0.0;
        for line in content.lines() {
            if line.contains("Sort by top of stack") {
                in_top_of_stack = true;
                continue;
            }
            if in_top_of_stack && line.trim().is_empty() {
                break;
            }

            if in_top_of_stack {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(count) = parts[0].parse::<u64>() {
                        let name = parts[1..].join(" ");
                        let pct = (count as f64 / total_samples as f64) * 100.0;
                        functions_guard.push(Function {
                            name,
                            module: "unknown".to_string(),
                            self_pct: (pct * 10.0).round() / 10.0,
                            total_pct: (pct * 10.0).round() / 10.0,
                            calls: count,
                            avg_ns: 0,
                            depth: 0,
                            x: ((current_x as f64) * 10.0).round() / 10.0,
                            w: (pct * 10.0).round() / 10.0,
                        });
                        current_x += pct;
                    }
                }
            }
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
