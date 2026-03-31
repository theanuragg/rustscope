use clap::Parser;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use anyhow::{Result, Context};
use chrono::Utc;
use std::sync::Arc;
use tokio::{signal, time::{sleep, Duration}};

mod profiler;
mod output;

use profiler::Profiler;

#[derive(Parser, Debug)]
#[command(author, version = "0.3.1", about = "Unified Rust performance profiler")]
struct Args {
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
