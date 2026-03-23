use clap::Parser;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use anyhow::{Result, Context};
use chrono::Utc;
use std::sync::Arc;
use tokio::signal;

mod profiler;
mod output;

use profiler::Profiler;

#[derive(Parser, Debug)]
#[command(author, version = "0.3.1", about = "Unified Rust performance profiler")]
struct Args {
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

    let (binary_path, binary_args, project_name) = if let Some(ref crate_name) = args.cargo {
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
        
        (binary_path, args.binary_args.clone(), target_bin.clone())
    } else {
        if args.binary_args.is_empty() {
            anyhow::bail!("No binary specified. Use -- <binary> [args...] or --cargo <crate>");
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
        (resolved_path, args.binary_args[1..].to_vec(), project_name)
    };

    if !binary_path.exists() {
        let msg = format!("Binary not found at: {:?}\n\nTips:\n- Use -- <absolute/relative/path/to/binary>\n- Use --cargo <crate> --bin <name> to build and run automatically\n- If running a local binary, ensure you prefix with './' (e.g., -- ./target/release/my-bin)", binary_path);
        anyhow::bail!(msg);
    }

    if args.duration > 0 {
        println!("Profiling {} for {}s...", project_name, args.duration);
    } else {
        println!("Profiling {} until process exits or Ctrl-C...", project_name);
    }

    let mut child = Command::new(&binary_path)
        .args(&binary_args)
        .stdout(if args.verbose { Stdio::inherit() } else { Stdio::null() })
        .stderr(if args.verbose { Stdio::inherit() } else { Stdio::null() })
        .spawn()
        .context(format!("Failed to spawn binary: {:?}", binary_path))?;

    let pid = child.id();
    let start_ts = Utc::now().timestamp_millis() as u64;
    let profiler = Arc::new(Profiler::new(pid, args.duration, args.sample_rate, args.verbose));

    let profiler_clone = Arc::clone(&profiler);
    let profiling_task = tokio::spawn(async move {
        profiler_clone.start().await
    });

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        println!("\nReceived Ctrl-C, shutting down...");
        let _ = shutdown_tx.send(());
    });

    tokio::select! {
        _ = profiling_task => {
            println!("Profiling duration completed.");
        }
        _ = &mut shutdown_rx => {
            // Profiler will stop on next tick as we'll drop the task or handle it via a stop flag
        }
    }

    // Ensure child is terminated
    let _ = child.kill();

    let output_path = args.output.unwrap_or_else(|| {
        "rustscope-last.json".to_string()
    });

    if PathBuf::from(&output_path).exists() {
        let _ = std::fs::remove_file(&output_path);
    }

    let results = profiler.collect_results(project_name, binary_path.to_string_lossy().to_string(), start_ts).await?;
    
    output::write_json(&output_path, &results)?;

    println!("✓ Profile written to {} ({} samples, {}s, {} events)", 
        output_path, results.samples.len(), results.meta.duration_sec, results.memory_events.len());

    Ok(())
}
