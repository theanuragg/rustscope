pub mod schema;

use std::fs::File;
use std::io::BufWriter;
use anyhow::Result;
use schema::{OutputSchema, Rollup};

pub fn write_json(path: &str, data: &OutputSchema) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, data)?;
    Ok(())
}

pub fn print_overview(path: &str, data: &OutputSchema) {
    let process_summary = data.process_summary.as_ref();
    let sampling_diagnostics = data.sampling_diagnostics.as_ref();
    let project_name = process_summary.map(|s| s.project.as_str()).unwrap_or("unknown");
    let target_binary = process_summary.map(|s| s.target_binary.as_str()).unwrap_or("unknown");
    let duration_sec = process_summary.map(|s| s.duration_sec).unwrap_or(data.session_duration_ns / 1_000_000_000);
    let cpu_avg_pct = process_summary.map(|s| s.cpu_avg_pct).unwrap_or(0.0);
    let cpu_peak_pct = process_summary.map(|s| s.cpu_peak_pct).unwrap_or(0.0);
    let heap_avg_mb = process_summary.map(|s| s.heap_avg_mb).unwrap_or(0.0);
    let heap_peak_mb = process_summary.map(|s| s.heap_peak_mb).unwrap_or(data.session_memory.peak_rss_mb);
    let thread_avg = process_summary.map(|s| s.thread_avg).unwrap_or(0);
    let fd_peak = process_summary.map(|s| s.fd_peak).unwrap_or(0);

    println!();
    println!("+------------------------------------------------------------------------------+");
    println!("| RustScope Session Overview                                                   |");
    println!("+------------------------------------------------------------------------------+");
    println!("  Output file     : {}", path);
    println!("  Target          : {}", project_name);
    println!("  Binary          : {}", target_binary);
    println!("  Host / cores    : {} / {}", data.host.os, data.host.cpu_logical_cores);
    println!("  Duration        : {}s", duration_sec);
    println!("  Samples         : {}", data.process_samples.len());
    println!("  Feature status  : samples={} events={} functions={}",
        yes_no(!data.process_samples.is_empty()),
        yes_no(!data.memory_events.is_empty()),
        yes_no(!data.functions.is_empty()),
    );
    if let Some(diag) = sampling_diagnostics {
        println!(
            "  Sampler         : {} | fidelity={} | fallback={}",
            diag.backend,
            diag.fidelity,
            yes_no(diag.fallback_used)
        );
    }
    println!();

    println!("  System metrics");
    println!("  --------------");
    println!(
        "  CPU avg / peak : {:.1}% / {:.1}%",
        cpu_avg_pct, cpu_peak_pct
    );
    println!(
        "  Heap avg / peak: {:.1} MB / {:.1} MB",
        heap_avg_mb, heap_peak_mb
    );
    println!(
        "  Threads avg    : {}",
        thread_avg
    );
    println!(
        "  FD peak        : {}",
        fd_peak
    );

    if !data.process_samples.is_empty() {
        let peak_syscalls = data.process_samples.iter().map(|sample| sample.syscalls_per_sec).max().unwrap_or(0);
        let peak_threads = data.process_samples.iter().map(|sample| sample.threads).max().unwrap_or(0);
        let peak_fds = data.process_samples.iter().map(|sample| sample.open_fds).max().unwrap_or(0);
        println!("  Peak syscalls/s: {}", peak_syscalls);
        println!("  Peak threads   : {}", peak_threads);
        println!("  Peak open FDs  : {}", peak_fds);
    }
    println!();

    println!("  Session events");
    println!("  --------------");
    if data.memory_events.is_empty() {
        println!("  No spikes detected in this session");
    } else {
        println!("  Total events    : {}", data.memory_events.len());
        for event in data.memory_events.iter().take(5) {
            println!("  - {:<12} {}", event.event_type, event.location);
        }
        if data.memory_events.len() > 5 {
            println!("  - ... and {} more", data.memory_events.len() - 5);
        }
    }
    println!();

    println!("  Hotspots");
    println!("  --------");
    if !data.functions.is_empty() {
        for (index, function) in data.functions.iter().take(8).enumerate() {
            let total_pct = function.timing.pct_of_session;
            let self_pct = if data.session_duration_ns > 0 {
                (function.timing.self_ns as f64 / data.session_duration_ns as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "  {:>2}. {:<28} total {:>5.1}% | self {:>5.1}% | calls {:>6}",
                index + 1,
                truncate(&function.name, 28),
                total_pct,
                self_pct,
                function.call_count
            );
        }
    } else {
        println!("  Function hotspots unavailable on this platform/profile mode");
    }
    println!();

    print_rollups("Crate rollups", &data.crate_rollups);
    print_rollups("Module rollups", &data.module_rollups);

    if let Some(diag) = sampling_diagnostics {
        println!("  Sampling diagnostics");
        println!("  --------------------");
        println!("  Backend         : {}", diag.backend);
        println!("  Fidelity        : {}", diag.fidelity);
        println!("  Raw samples     : {}", diag.raw_samples);
        println!("  Symbolized      : {}", diag.symbolized_samples);
        println!("  Dropped         : {}", diag.dropped_samples);
        println!("  Unknown symbols : {}", diag.unknown_symbols);
        println!();
    }

    println!("  Notes");
    println!("  -----");
    if data.functions.is_empty() {
        println!("  - JSON is valid, but stack/function sampling did not populate hotspots.");
    }
    if cpu_peak_pct < 10.0 {
        println!("  - CPU peak was low; longer or heavier session traffic may reveal more signal.");
    }
    if data.memory_events.is_empty() {
        println!("  - No memory/CPU spikes crossed the event threshold during this session.");
    }
    println!("+------------------------------------------------------------------------------+");
}

pub struct LiveSnapshot {
    pub target: String,
    pub pid: u32,
    pub elapsed_secs: u64,
    pub samples: usize,
    pub compact: bool,
    pub refresh_ms: u64,
    pub current_cpu_pct: f64,
    pub peak_cpu_pct: f64,
    pub current_heap_mb: f64,
    pub peak_heap_mb: f64,
    pub current_threads: u32,
    pub peak_threads: u32,
    pub current_fds: u32,
    pub peak_fds: u32,
    pub current_syscalls_per_sec: u64,
    pub peak_syscalls_per_sec: u64,
    pub event_count: usize,
    pub last_event: Option<String>,
    pub recent_events: Vec<String>,
}

pub fn print_live_dashboard(snapshot: &LiveSnapshot) {
    print!("\x1b[2J\x1b[H");
    println!("+------------------------------------------------------------------------------+");
    println!("| RustScope Live Session                                                       |");
    println!("+------------------------------------------------------------------------------+");
    println!("  Target         : {} (pid {})", snapshot.target, snapshot.pid);
    println!("  Elapsed        : {}s", snapshot.elapsed_secs);
    println!("  Samples        : {}", snapshot.samples);
    println!("  Events         : {}", snapshot.event_count);
    println!("  Mode           : {}", if snapshot.compact { "compact" } else { "full" });
    println!("  Refresh        : {} ms", snapshot.refresh_ms);
    println!("  Controls       : q=quit c=compact s=slow f=fast + Enter");
    println!();
    if snapshot.compact {
        println!("  CPU {:>6.1}% | HEAP {:>6.1} MB | THR {:>4} | FD {:>4} | SYS {:>5}",
            snapshot.current_cpu_pct,
            snapshot.current_heap_mb,
            snapshot.current_threads,
            snapshot.current_fds,
            snapshot.current_syscalls_per_sec
        );
        println!("  Peak CPU {:>6.1}% | Peak heap {:>6.1} MB | Peak thr {:>4} | Peak fd {:>4}",
            snapshot.peak_cpu_pct,
            snapshot.peak_heap_mb,
            snapshot.peak_threads,
            snapshot.peak_fds
        );
        println!();
        println!("  Latest event   : {}", snapshot.last_event.as_deref().unwrap_or("none"));
        return;
    }

    println!("  Current metrics");
    println!("  ---------------");
    println!("  CPU            : {:>6.1}%", snapshot.current_cpu_pct);
    println!("  Heap           : {:>6.1} MB", snapshot.current_heap_mb);
    println!("  Threads        : {:>6}", snapshot.current_threads);
    println!("  Open FDs       : {:>6}", snapshot.current_fds);
    println!("  Syscalls/s     : {:>6}", snapshot.current_syscalls_per_sec);
    println!();
    println!("  Session peaks");
    println!("  -------------");
    println!("  Peak CPU       : {:>6.1}%", snapshot.peak_cpu_pct);
    println!("  Peak heap      : {:>6.1} MB", snapshot.peak_heap_mb);
    println!("  Peak threads   : {:>6}", snapshot.peak_threads);
    println!("  Peak open FDs  : {:>6}", snapshot.peak_fds);
    println!("  Peak syscalls/s: {:>6}", snapshot.peak_syscalls_per_sec);
    println!();
    println!("  Latest event");
    println!("  ------------");
    match &snapshot.last_event {
        Some(event) => println!("  {}", event),
        None => println!("  none"),
    }
    println!();
    println!("  Event log");
    println!("  ---------");
    if snapshot.recent_events.is_empty() {
        println!("  none");
    } else {
        for event in &snapshot.recent_events {
            println!("  - {}", event);
        }
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.len() <= width {
        return value.to_string();
    }
    let cutoff = width.saturating_sub(3);
    format!("{}...", &value[..cutoff])
}

fn yes_no(enabled: bool) -> &'static str {
    if enabled { "yes" } else { "no" }
}

fn print_rollups(title: &str, rollups: &[Rollup]) {
    println!("  {}", title);
    println!("  {}", "-".repeat(title.len()));
    if rollups.is_empty() {
        println!("  none");
        println!();
        return;
    }

    for (index, rollup) in rollups.iter().take(6).enumerate() {
        println!(
            "  {:>2}. {:<28} total {:>5.1}% | self {:>5.1}% | calls {:>6} | fns {:>3}",
            index + 1,
            truncate(&rollup.name, 28),
            rollup.total_pct,
            rollup.self_pct,
            rollup.calls,
            rollup.function_count
        );
    }
    println!();
}
