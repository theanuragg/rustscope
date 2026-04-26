use clap::Parser;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use anyhow::{Result, Context};
use chrono::Utc;
use std::sync::Arc;
use tokio::{signal, time::{sleep, Duration}};
use rustscope::diff::{DiffConfig, SessionDiff};

mod profiler;
mod output;

use profiler::Profiler;

#[derive(Parser, Debug)]
#[command(author, version = "0.3.1", about = "Unified Rust performance profiler")]
struct Args {
    /// Check sampler/backend readiness and exit
    #[arg(long)]
    doctor: bool,

    /// Compare two profile JSON files instead of collecting a new session
    #[arg(long)]
    compare_baseline: Option<String>,

    /// Current profile JSON for compare mode
    #[arg(long)]
    compare_current: Option<String>,

    /// Output path for compare diff JSON
    #[arg(long)]
    compare_output: Option<String>,

    /// Regression threshold percentage for compare mode
    #[arg(long, default_value_t = 10.0)]
    threshold: f64,

    /// Fail compare mode on: any, minor, moderate, critical
    #[arg(long, default_value = "critical")]
    fail_on: String,

    /// Attach to an already-running process ID instead of spawning a child
    #[arg(long)]
    pid: Option<u32>,

    /// Optional display name for attached session profiling
    #[arg(long)]
    name: Option<String>,

    /// How long to profile in seconds (0 for indefinite)
    #[arg(short, long, default_value_t = 0)]
    duration: u32,

    /// Samples per second
    #[arg(short, long, default_value_t = 100)]
    sample_rate: u32,

    /// Output JSON path
    #[arg(short, long)]
    output: Option<String>,

    /// Build and profile a cargo project
    #[arg(long)]
    cargo: Option<String>,

    /// Specific binary to run if the crate has multiple
    #[arg(long)]
    bin: Option<String>,

    /// Skip cargo build step
    #[arg(long)]
    no_build: bool,

    /// Show live metrics while running
    #[arg(short, long)]
    verbose: bool,

    /// Binary to profile and its arguments
    #[arg(last = true)]
    binary_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.doctor {
        run_doctor()?;
        return Ok(());
    }

    if let (Some(baseline), Some(current)) = (&args.compare_baseline, &args.compare_current) {
        run_compare_mode(baseline, current, args.compare_output.as_deref(), args.threshold, &args.fail_on)?;
        return Ok(());
    }

    let (target_pid, binary_path, binary_args, project_name, spawned_child) = if let Some(pid) = args.pid {
        let project_name = args.name.clone().unwrap_or_else(|| format!("pid-{}", pid));
        (pid, None, Vec::new(), project_name, false)
    } else if let Some(ref crate_name) = args.cargo {
        if !args.no_build {
            println!("Building project {} in release mode...", crate_name);
            let mut cmd = Command::new("cargo");
            cmd.args(["build", "--release", "-p", crate_name]);
            if let Some(ref bin_name) = args.bin {
                cmd.args(["--bin", bin_name]);
            }
            let status = cmd.status().context("Failed to run cargo build")?;
            if !status.success() {
                anyhow::bail!("Cargo build failed");
            }
        }
        
        // Use the bin name if provided, otherwise assume crate name
        let target_bin = args.bin.as_ref().unwrap_or(crate_name);
        
        // Smarter binary finding: check local target and parent target (for workspace)
        let binary_path = if PathBuf::from("target/release").join(target_bin).exists() {
            PathBuf::from("target/release").join(target_bin)
        } else if PathBuf::from("../target/release").join(target_bin).exists() {
            PathBuf::from("../target/release").join(target_bin)
        } else {
            // Fallback to searching up for a 'target' directory
            let mut current = std::env::current_dir()?;
            let mut found = None;
            loop {
                let target = current.join("target/release").join(target_bin);
                if target.exists() {
                    found = Some(target);
                    break;
                }
                if !current.pop() {
                    break;
                }
            }
            found.unwrap_or_else(|| PathBuf::from("target/release").join(target_bin))
        };
        
        (0, Some(binary_path), args.binary_args.clone(), target_bin.clone(), true)
    } else {
        if args.binary_args.is_empty() {
            anyhow::bail!("No target specified. Use --pid <PID>, -- <binary> [args...], or --cargo <crate>");
        }
        let binary_path_str = &args.binary_args[0];
        let binary_path = PathBuf::from(binary_path_str);
        
        // If the path doesn't exist and looks like a name, check target/release
        let resolved_path = if !binary_path.exists() && !binary_path_str.contains('/') {
            let mut current = std::env::current_dir()?;
            let mut found = None;
            loop {
                let target = current.join("target/release").join(binary_path_str);
                if target.exists() {
                    found = Some(target);
                    break;
                }
                if !current.pop() {
                    break;
                }
            }
            found.unwrap_or(binary_path)
        } else {
            binary_path
        };
        
        let project_name = resolved_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        (0, Some(resolved_path), args.binary_args[1..].to_vec(), project_name, true)
    };

    if let Some(ref binary_path) = binary_path {
        if !binary_path.exists() {
            let msg = format!("Binary not found at: {:?}\n\nTips:\n- Use -- <absolute/relative/path/to/binary>\n- Use --cargo <crate> --bin <name> to build and run automatically\n- Use --pid <PID> to attach to an already-running Rust service\n- If running a local binary, ensure you prefix with './' (e.g., -- ./target/release/my-bin)", binary_path);
            anyhow::bail!(msg);
        }
    }

    if args.pid.is_some() && args.duration == 0 {
        println!("Profiling session {} until Ctrl-C or process exit...", project_name);
    } else if args.duration > 0 {
        println!("Profiling {} for {}s...", project_name, args.duration);
    } else {
        println!("Profiling {} until process exits or Ctrl-C...", project_name);
    }

    let child = if let Some(ref binary_path) = binary_path {
        Some(Command::new(binary_path)
            .args(&binary_args)
            .stdout(if args.verbose { Stdio::inherit() } else { Stdio::null() })
            .stderr(if args.verbose { Stdio::inherit() } else { Stdio::null() })
            .spawn()
            .context(format!("Failed to spawn binary: {:?}", binary_path))?)
    } else {
        None
    };

    let pid = child.as_ref().map(|c| c.id()).unwrap_or(target_pid);
    let start_ts = Utc::now().timestamp_millis() as u64;
    let profiler = Arc::new(Profiler::new(pid, args.duration, args.sample_rate, args.verbose));

    if args.verbose {
        spawn_live_controls(Arc::clone(&profiler));
    }

    let profiler_clone = Arc::clone(&profiler);
    let profiling_task = tokio::spawn(async move {
        profiler_clone.start().await
    });
    tokio::pin!(profiling_task);

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        println!("\nReceived Ctrl-C, shutting down...");
        let _ = shutdown_tx.send(());
    });

    let mut finished_naturally = false;
    tokio::select! {
        result = &mut profiling_task => {
            result??;
            finished_naturally = true;
            println!("Profiling session completed.");
        }
        _ = &mut shutdown_rx => {
            profiler.stop();
        }
    }

    if !finished_naturally {
        sleep(Duration::from_millis(150)).await;
        let result = (&mut profiling_task).await;
        result??;
        println!("Profiling session completed.");
    }

    if let Some(mut child) = child {
        if spawned_child {
            let _ = child.kill();
        }
    }

    profiler.stop();

    let output_path = args.output.unwrap_or_else(|| {
        "rustscope-last.json".to_string()
    });

    if PathBuf::from(&output_path).exists() {
        let _ = std::fs::remove_file(&output_path);
    }

    let target_binary = if let Some(ref binary_path) = binary_path {
        binary_path.to_string_lossy().to_string()
    } else {
        format!("pid:{}", pid)
    };

    let results = profiler.collect_results(project_name, target_binary, start_ts).await?;
    
    output::write_json(&output_path, &results)?;
    output::print_overview(&output_path, &results);

    let final_duration_sec = results
        .process_summary
        .as_ref()
        .map(|summary| summary.duration_sec)
        .unwrap_or(results.session_duration_ns / 1_000_000_000);
    println!("✓ Profile written to {} ({} samples, {}s, {} events)", 
        output_path, results.process_samples.len(), final_duration_sec, results.memory_events.len());

    Ok(())
}

fn run_doctor() -> Result<()> {
    println!("RustScope Doctor");
    println!("----------------");
    println!("platform          : {}", std::env::consts::OS);
    println!("arch              : {}", std::env::consts::ARCH);
    println!("cores             : {}", num_cpus::get());

    #[cfg(target_os = "macos")]
    {
        let sample_ok = command_exists("sample");
        println!("backend           : macos-sample");
        println!("sampler available : {}", yes_no(sample_ok));
        println!("debug symbols     : unknown");
        println!(
            "symbolization     : {}",
            if sample_ok { "expected-medium" } else { "unavailable" }
        );
        println!(
            "recommended action: {}",
            if sample_ok {
                "build with debuginfo and run a longer session for better stack fidelity"
            } else {
                "install Xcode command line tools and ensure /usr/bin/sample is available"
            }
        );
    }

    #[cfg(target_os = "linux")]
    {
        let perf_ok = command_ok("perf", &["--version"]);
        println!("backend           : linux-perf");
        println!("sampler available : {}", yes_no(perf_ok));
        println!("debug symbols     : unknown");
        println!(
            "symbolization     : {}",
            if perf_ok { "expected-medium" } else { "unavailable" }
        );
        println!(
            "recommended action: {}",
            if perf_ok {
                "build with debuginfo=2 and verify perf_event permissions"
            } else {
                "install linux perf tools and verify perf_event access"
            }
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("backend           : unsupported");
        println!("sampler available : no");
        println!("recommended action: use macOS or Linux for stack sampling");
    }

    Ok(())
}

fn command_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn spawn_live_controls(profiler: Arc<Profiler>) {
    std::thread::spawn(move || {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            match line.trim() {
                "q" | "quit" => {
                    profiler.stop();
                    break;
                }
                "c" | "compact" => profiler.toggle_compact_mode(),
                "s" | "slow" => profiler.set_refresh_ms(3000),
                "f" | "fast" => profiler.set_refresh_ms(500),
                _ => {}
            }
        }
    });
}

fn run_compare_mode(
    baseline_path: &str,
    current_path: &str,
    output_path: Option<&str>,
    threshold: f64,
    fail_on: &str,
) -> Result<()> {
    let baseline = SessionDiff::load_session(baseline_path)?;
    let current = SessionDiff::load_session(current_path)?;
    let config = DiffConfig {
        regression_threshold_pct: threshold,
        ..Default::default()
    };
    let diff = SessionDiff::compare(&baseline, &current, &config, baseline_path, current_path);

    if let Some(output_path) = output_path {
        diff.save_json(output_path)?;
        println!("Diff written to {}", output_path);
    }

    println!("{}", diff.summary_text());

    let should_fail = match fail_on {
        "any" => diff.has_any_regressions(),
        "minor" => diff.summary.regressions_minor > 0 || diff.summary.regressions_moderate > 0 || diff.summary.regressions_critical > 0,
        "moderate" => diff.summary.regressions_moderate > 0 || diff.summary.regressions_critical > 0,
        "critical" => diff.has_critical_regressions(),
        _ => diff.has_critical_regressions(),
    };

    if should_fail {
        anyhow::bail!("Regression threshold exceeded for fail-on={}", fail_on);
    }

    Ok(())
}
