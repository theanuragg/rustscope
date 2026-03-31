use anyhow::Result;
use std::time::Duration;
use crate::output::{
    print_live_dashboard,
    LiveSnapshot,
    schema::{Sample, OutputSchema, Summary, MemoryEvent, Rollup}
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use chrono::Utc;
use std::collections::HashMap;

mod cpu;
mod memory;
mod threads_fds;
mod syscalls;
mod functions;

use cpu::CpuCollector;
use memory::MemoryCollector;
use threads_fds::ThreadFdCollector;
use syscalls::SyscallCollector;
use functions::FunctionSampler;

pub struct Profiler {
    pid: u32,
    duration: Duration,
    sample_rate: u32,
    verbose: bool,
    samples: Arc<Mutex<Vec<Sample>>>,
    memory_events: Arc<Mutex<Vec<MemoryEvent>>>,
    cpu_collector: Mutex<CpuCollector>,
    mem_collector: MemoryCollector,
    threads_fds_collector: ThreadFdCollector,
    syscall_collector: Mutex<SyscallCollector>,
    function_sampler: Arc<FunctionSampler>,
    stop_requested: AtomicBool,
    started_at: std::time::Instant,
    last_dashboard_refresh_secs: Mutex<u64>,
}

impl Profiler {
    pub fn new(pid: u32, duration: u32, sample_rate: u32, verbose: bool) -> Self {
        Self {
            pid,
            duration: Duration::from_secs(duration as u64),
            sample_rate,
            verbose,
            samples: Arc::new(Mutex::new(Vec::new())),
            memory_events: Arc::new(Mutex::new(Vec::new())),
            cpu_collector: Mutex::new(CpuCollector::new(pid)),
            mem_collector: MemoryCollector::new(pid),
            threads_fds_collector: ThreadFdCollector::new(pid),
            syscall_collector: Mutex::new(SyscallCollector::new(pid)),
            function_sampler: Arc::new(FunctionSampler::new(pid)),
            stop_requested: AtomicBool::new(false),
            started_at: std::time::Instant::now(),
            last_dashboard_refresh_secs: Mutex::new(u64::MAX),
        }
    }

    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    pub async fn start(&self) -> Result<()> {
        let duration_secs = self.duration.as_secs() as u32;
        let function_sampler_clone = Arc::clone(&self.function_sampler);
        
        // Start function sampling in background
        let function_task = tokio::spawn(async move {
            let _ = function_sampler_clone.start_sampling(duration_secs).await;
        });

        let mut interval = tokio::time::interval(Duration::from_millis(1000 / self.sample_rate as u64));
        let start_time = std::time::Instant::now();

        loop {
            if self.stop_requested.load(Ordering::SeqCst) {
                break;
            }

            // Check if duration exceeded (if duration > 0)
            if self.duration.as_secs() > 0 && start_time.elapsed() >= self.duration {
                break;
            }

            interval.tick().await;
            
            // If collection fails, it usually means the process is gone
            if let Err(e) = self.collect_sample(self.verbose).await {
                if e.to_string().contains("Process not found") {
                    break;
                }
                return Err(e);
            }
        }

        // Wait for function sampling task to finish (it will finish after the child exits or duration expires)
        let _ = function_task.await;

        Ok(())
    }

    async fn collect_sample(&self, verbose: bool) -> Result<()> {
        let cpu_pct = self.collect_cpu().await?;
        let heap_mb = self.collect_memory()?;
        let threads = self.collect_threads()?;
        let open_fds = self.collect_fds()?;
        let syscalls_per_sec = self.syscall_collector.lock().await.collect().unwrap_or(0);

        let mut samples = self.samples.lock().await;
        let last_heap_mb = samples.last().map(|s| s.heap_mb).unwrap_or(heap_mb);
        let ts = Utc::now().timestamp_millis() as u64;

        // Spike detection: heap grew >5 MB or CPU > 80%
        let mut event_occurred = false;
        if heap_mb > last_heap_mb + 5.0 {
            let mut events = self.memory_events.lock().await;
            events.push(MemoryEvent {
                ts,
                event_type: "spike".to_string(),
                size_bytes: ((heap_mb - last_heap_mb) * 1024.0 * 1024.0) as u64,
                location: format!("Memory spike: +{:.1} MB", heap_mb - last_heap_mb),
            });
            event_occurred = true;
        }
        
        if cpu_pct > 80.0 {
            let mut events = self.memory_events.lock().await;
            events.push(MemoryEvent {
                ts,
                event_type: "cpu_spike".to_string(),
                size_bytes: 0,
                location: format!("CPU spike: {:.1}%", cpu_pct),
            });
            event_occurred = true;
        }

        samples.push(Sample {
            ts,
            cpu_pct: (cpu_pct * 10.0).round() / 10.0,
            heap_mb: (heap_mb * 10.0).round() / 10.0,
            threads,
            open_fds,
            syscalls_per_sec,
        });

        let elapsed_secs = self.started_at.elapsed().as_secs();
        if verbose {
            self.maybe_render_dashboard(
                elapsed_secs,
                cpu_pct,
                heap_mb,
                threads,
                open_fds,
                syscalls_per_sec,
                &samples,
                event_occurred
            ).await;
        }

        Ok(())
    }

    pub async fn collect_results(&self, project: String, target_binary: String, start_ts: u64) -> Result<OutputSchema> {
        let samples = self.samples.lock().await;
        let memory_events = self.memory_events.lock().await;
        let mut sampled_functions = self.function_sampler.get_functions().await;
        let end_ts = Utc::now().timestamp_millis() as u64;

        // Calculate summary stats
        let cpu_avg_pct = if samples.is_empty() { 0.0 } else { samples.iter().map(|s| s.cpu_pct).sum::<f64>() / samples.len() as f64 };
        let cpu_peak_pct = samples.iter().map(|s| s.cpu_pct).fold(0.0, f64::max);
        let heap_avg_mb = if samples.is_empty() { 0.0 } else { samples.iter().map(|s| s.heap_mb).sum::<f64>() / samples.len() as f64 };
        let heap_peak_mb = samples.iter().map(|s| s.heap_mb).fold(0.0, f64::max);
        let thread_avg = if samples.is_empty() { 0 } else { (samples.iter().map(|s| s.threads).sum::<u32>() as f64 / samples.len() as f64).round() as u32 };
        let fd_peak = samples.iter().map(|s| s.open_fds).max().unwrap_or(0);
        let duration_sec = ((end_ts.saturating_sub(start_ts)) / 1000).max(1);
        let session_duration_ns = end_ts.saturating_sub(start_ts) * 1_000_000;
        if sampled_functions.is_empty() {
            sampled_functions.push(crate::output::schema::Function {
                name: project.clone(),
                module: project.clone(),
                self_pct: 100.0,
                total_pct: 100.0,
                calls: samples.len() as u64,
                avg_ns: if samples.is_empty() { 0 } else { (session_duration_ns / samples.len() as u64) },
                depth: 0,
                x: 0.0,
                w: 100.0,
            });
        }
        let crate_rollups = build_rollups(&sampled_functions, RollupKind::Crate);
        let module_rollups = build_rollups(&sampled_functions, RollupKind::Module);
        let functions = convert_functions(&sampled_functions, session_duration_ns);

        Ok(OutputSchema {
            schema_version: 3,
            started_at_unix_secs: start_ts / 1000,
            session_duration_ns,
            host: rustscope::output::schema::HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                cpu_logical_cores: num_cpus::get() as u32,
                rustc_version: "unknown".to_string(),
                build_profile: "release".to_string(),
            },
            functions,
            benchmarks: Vec::new(),
            call_trees: Vec::new(),
            session_memory: rustscope::output::schema::SessionMemory {
                total_alloc_bytes: 0,
                total_dealloc_bytes: 0,
                peak_rss_mb: (heap_peak_mb * 10.0).round() / 10.0,
                peak_heap_bytes: (heap_peak_mb * 1024.0 * 1024.0) as u64,
                final_heap_bytes: samples.last().map(|sample| (sample.heap_mb * 1024.0 * 1024.0) as u64).unwrap_or(0),
                total_alloc_ops: 0,
            },
            async_spans: None,
            thread_report: None,
            session_meta: Some(serde_json::json!({
                "project": project,
                "target_binary": target_binary,
                "start_ts_ms": start_ts,
                "end_ts_ms": end_ts,
                "rustscope_version": "0.3.1",
            })),
            outlier_summary: None,
            timeline_summary: None,
            locks: Vec::new(),
            async_tasks: Vec::new(),
            process_summary: Some(Summary {
                cpu_avg_pct: (cpu_avg_pct * 10.0).round() / 10.0,
                cpu_peak_pct: (cpu_peak_pct * 10.0).round() / 10.0,
                heap_avg_mb: (heap_avg_mb * 10.0).round() / 10.0,
                heap_peak_mb: (heap_peak_mb * 10.0).round() / 10.0,
                thread_avg,
                fd_peak,
                total_allocations: 0,
                total_deallocations: 0,
                leaked_bytes: 0,
                project,
                target_binary,
                duration_sec,
            }),
            process_samples: samples.clone(),
            memory_events: memory_events.clone(),
            crate_rollups,
            module_rollups,
        })
    }

    async fn collect_cpu(&self) -> Result<f64> {
        match self.cpu_collector.lock().await.collect() {
            Ok(value) => Ok(value),
            Err(_err) if !self.process_exists() => anyhow::bail!("Process not found"),
            Err(_) => Ok(0.0),
        }
    }

    fn collect_memory(&self) -> Result<f64> {
        match self.mem_collector.collect() {
            Ok(value) => Ok(value),
            Err(_err) if !self.process_exists() => anyhow::bail!("Process not found"),
            Err(_) => Ok(0.0),
        }
    }

    fn collect_threads(&self) -> Result<u32> {
        match self.threads_fds_collector.collect_threads() {
            Ok(value) => Ok(value),
            Err(_err) if !self.process_exists() => anyhow::bail!("Process not found"),
            Err(_) => Ok(0),
        }
    }

    fn collect_fds(&self) -> Result<u32> {
        match self.threads_fds_collector.collect_fds() {
            Ok(value) => Ok(value),
            Err(_err) if !self.process_exists() => anyhow::bail!("Process not found"),
            Err(_) => Ok(0),
        }
    }

    fn process_exists(&self) -> bool {
        #[cfg(unix)]
        {
            let pid = nix::unistd::Pid::from_raw(self.pid as i32);
            nix::sys::signal::kill(pid, None).is_ok()
        }

        #[cfg(not(unix))]
        {
            true
        }
    }

    async fn maybe_render_dashboard(
        &self,
        elapsed_secs: u64,
        current_cpu_pct: f64,
        current_heap_mb: f64,
        current_threads: u32,
        current_fds: u32,
        current_syscalls_per_sec: u64,
        samples: &[Sample],
        event_occurred: bool,
    ) {
        let mut last_refresh = self.last_dashboard_refresh_secs.lock().await;
        if *last_refresh == elapsed_secs && !event_occurred {
            return;
        }
        *last_refresh = elapsed_secs;

        let peak_cpu_pct = samples.iter().map(|sample| sample.cpu_pct).fold(0.0, f64::max);
        let peak_heap_mb = samples.iter().map(|sample| sample.heap_mb).fold(0.0, f64::max);
        let peak_threads = samples.iter().map(|sample| sample.threads).max().unwrap_or(0);
        let peak_fds = samples.iter().map(|sample| sample.open_fds).max().unwrap_or(0);
        let peak_syscalls_per_sec = samples.iter().map(|sample| sample.syscalls_per_sec).max().unwrap_or(0);
        let memory_events = self.memory_events.lock().await;
        let event_count = memory_events.len();
        let last_event = memory_events.last().map(|event| format!("{} | {}", event.event_type, event.location));

        print_live_dashboard(&LiveSnapshot {
            target: if self.pid > 0 { format!("pid-{}", self.pid) } else { "session".to_string() },
            pid: self.pid,
            elapsed_secs,
            samples: samples.len(),
            current_cpu_pct,
            peak_cpu_pct,
            current_heap_mb,
            peak_heap_mb,
            current_threads,
            peak_threads,
            current_fds,
            peak_fds,
            current_syscalls_per_sec,
            peak_syscalls_per_sec,
            event_count,
            last_event,
        });
    }
}

enum RollupKind {
    Crate,
    Module,
}

fn build_rollups(functions: &[crate::output::schema::Function], kind: RollupKind) -> Vec<Rollup> {
    let mut grouped: HashMap<String, Rollup> = HashMap::new();

    for function in functions {
        let key = match kind {
            RollupKind::Crate => derive_crate(&function.name, &function.module),
            RollupKind::Module => derive_module(&function.name, &function.module),
        };

        let entry = grouped.entry(key.clone()).or_insert(Rollup {
            name: key,
            total_pct: 0.0,
            self_pct: 0.0,
            calls: 0,
            function_count: 0,
        });

        entry.total_pct += function.total_pct;
        entry.self_pct += function.self_pct;
        entry.calls += function.calls;
        entry.function_count += 1;
    }

    let mut rollups: Vec<_> = grouped.into_values().collect();
    rollups.sort_by(|a, b| b.total_pct.partial_cmp(&a.total_pct).unwrap_or(std::cmp::Ordering::Equal));
    rollups
}

fn convert_functions(functions: &[crate::output::schema::Function], session_duration_ns: u64) -> Vec<rustscope::output::schema::FunctionRecord> {
    functions.iter().map(|function| {
        let total_ns = ((function.total_pct / 100.0) * session_duration_ns as f64).round() as u64;
        let self_ns = ((function.self_pct / 100.0) * session_duration_ns as f64).round() as u64;
        rustscope::output::schema::FunctionRecord {
            name: function.name.clone(),
            module_path: function.module.clone(),
            file: String::new(),
            line: 0,
            call_count: function.calls,
            max_recursion_depth: 1,
            timing: rustscope::output::schema::TimingStats {
                total_ns,
                self_ns,
                avg_ns: function.avg_ns,
                min_ns: 0,
                max_ns: 0,
                p95_ns: 0,
                p99_ns: 0,
                pct_of_session: function.total_pct,
                mean_ns: function.avg_ns as f64,
                stddev_ns: 0.0,
                p50_ns: 0,
            },
            memory: None,
            stack: rustscope::output::schema::StackMetrics {
                max_depth: function.depth,
                avg_depth: function.depth as f64,
                frame_size_bytes: None,
                max_call_depth: function.depth,
            },
            cpu: None,
            outlier_count: 0,
        }
    }).collect()
}

fn derive_crate(name: &str, module: &str) -> String {
    let source = if module != "unknown" { module } else { name };
    source.split("::").next().unwrap_or("unknown").to_string()
}

fn derive_module(name: &str, module: &str) -> String {
    if module != "unknown" {
        return module.to_string();
    }

    let parts: Vec<&str> = name.split("::").filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [] => "unknown".to_string(),
        [single] => (*single).to_string(),
        [first, second, ..] => format!("{}::{}", first, second),
    }
}
