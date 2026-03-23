use anyhow::Result;
use std::time::Duration;
use crate::output::schema::{Sample, OutputSchema, Meta, Summary, Allocations, BySize, MemoryEvent};
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;

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
        }
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
        let cpu_pct = self.cpu_collector.lock().await.collect()?;
        let heap_mb = self.mem_collector.collect()?;
        let threads = self.threads_fds_collector.collect_threads()?;
        let open_fds = self.threads_fds_collector.collect_fds()?;
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

        if verbose {
            let events_count = self.memory_events.lock().await.len();
            let event_indicator = if event_occurred { " [!] SPIKE" } else { "" };
            print!("\r\x1b[K[LIVE] CPU: {:>5.1}% | MEM: {:>7.1} MB | THREADS: {:>3} | FDs: {:>4} | EVENTS: {:>2}{}", 
                   cpu_pct, heap_mb, threads, open_fds, events_count, event_indicator);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }

        samples.push(Sample {
            ts,
            cpu_pct: (cpu_pct * 10.0).round() / 10.0,
            heap_mb: (heap_mb * 10.0).round() / 10.0,
            threads,
            open_fds,
            syscalls_per_sec,
        });

        Ok(())
    }

    pub async fn collect_results(&self, project: String, target_binary: String, start_ts: u64) -> Result<OutputSchema> {
        let samples = self.samples.lock().await;
        let memory_events = self.memory_events.lock().await;
        let functions = self.function_sampler.get_functions().await;
        let end_ts = Utc::now().timestamp_millis() as u64;

        // Calculate summary stats
        let cpu_avg_pct = if samples.is_empty() { 0.0 } else { samples.iter().map(|s| s.cpu_pct).sum::<f64>() / samples.len() as f64 };
        let cpu_peak_pct = samples.iter().map(|s| s.cpu_pct).fold(0.0, f64::max);
        let heap_avg_mb = if samples.is_empty() { 0.0 } else { samples.iter().map(|s| s.heap_mb).sum::<f64>() / samples.len() as f64 };
        let heap_peak_mb = samples.iter().map(|s| s.heap_mb).fold(0.0, f64::max);
        let thread_avg = if samples.is_empty() { 0 } else { (samples.iter().map(|s| s.threads).sum::<u32>() as f64 / samples.len() as f64).round() as u32 };
        let fd_peak = samples.iter().map(|s| s.open_fds).max().unwrap_or(0);

        Ok(OutputSchema {
            meta: Meta {
                project,
                duration_sec: self.duration.as_secs(),
                start_ts,
                end_ts,
                rustscope_version: "0.3.1".to_string(),
                target_binary,
                host_os: std::env::consts::OS.to_string(),
                cpu_cores: num_cpus::get() as u32,
            },
            summary: Summary {
                cpu_avg_pct: (cpu_avg_pct * 10.0).round() / 10.0,
                cpu_peak_pct: (cpu_peak_pct * 10.0).round() / 10.0,
                heap_avg_mb: (heap_avg_mb * 10.0).round() / 10.0,
                heap_peak_mb: (heap_peak_mb * 10.0).round() / 10.0,
                thread_avg,
                fd_peak,
                total_allocations: 0,
                total_deallocations: 0,
                leaked_bytes: 0,
            },
            samples: samples.clone(),
            functions,
            allocations: Allocations {
                by_size: BySize {
                    range_0_64b: 0,
                    range_65_512b: 0,
                    range_513b_4kb: 0,
                    range_4kb_64kb: 0,
                    range_gt_64kb: 0,
                },
            },
            memory_events: memory_events.clone(),
        })
    }
}
