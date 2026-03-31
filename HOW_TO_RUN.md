# How to Run RustScope

## Prerequisites
```bash
# Rust toolchain (1.70+)
curl https://sh.rustup.rs -sSf | sh

# Clone / unzip the project
cd rustscope
```

---

## 1. Quickest demo — features_demo (all v3 features)
```bash
# Run the binary directly (metrics will be printed to terminal by the app itself)
cargo run --release -p rustscope-examples --bin features_demo
```
**What you see:**
- Terminal table of every profiled function (timing, memory, call count)
- Outlier detections printed live as functions spike

---

## 2. Using the RustScope Profiler CLI
The `rustscope` command is a separate tool that monitors **any** binary from the outside. Use this when you want a unified JSON report of system-level metrics (CPU, Memory, FDs, Threads).

```bash
# Install the profiler CLI like a normal crate
cargo install --path rustscope-cli

# Profile a workload that triggers CPU, memory, threads, FDs, and syscalls
rustscope -v --cargo rustscope-examples --bin stress_demo

# Or attach to an already-running Rust service and profile the full session
rustscope --pid 12345 --name my-api
```

**Why use the Profiler for Backends/Servers?**
- **Indefinite Monitoring**: By default, it runs as long as your server is alive.
- **Session Profiling**: `--pid <PID>` lets you attach to a running Rust process and collect one full session until `Ctrl-C`.
- **Spike Detection**: If you hit a route and cause a CPU/Memory spike, it will be captured as a "Session Event".
- **Live Event Count**: The terminal shows a real-time count of detected spikes (`EVENTS: N [!] SPIKE`).
- **Unified Output**: Writes everything to `rustscope-last.json` and prints a terminal overview with top metrics.
- **Graceful Shutdown**: Hit `Ctrl-C` after your testing session; all spikes/metrics are flushed to the JSON.

---

## 3. CLI profiler — profile any binary

```bash
# Build the profiler CLI
cargo build --release -p rustscope-cli
alias rustscope=./target/release/rustscope

# Profile a binary with default duration (30s)
rustscope -- ./target/release/profile_demo

# Profile a cargo project directly (auto-builds in release)
rustscope --cargo rustscope-examples --bin advanced_demo -d 5

# Inspect the output JSON
cat rustscope-<timestamp>.json
```

## 4. Analysis CLI — inspect any profile.json
(Note: This part is from the legacy rustscope-cli tool if you want to use its analysis features)
```bash
# Run a demo to generate a profile
cargo run --release -p rustscope-examples --bin profile_demo
```

---

## 4. Integration tests

```bash
cargo test -p rustscope             # 27 tests, ~3 seconds
cargo test -p rustscope -- --nocapture  # see test output
```

---

## 5. jq queries on any profile.json

```bash
# Top 5 slowest functions by p99
jq '.functions | sort_by(-.timing.p99_ns) | .[0:5] |
    .[] | {name, p99_ms: (.timing.p99_ns/1e6)}' profile.json

# Functions with outlier calls
jq '.functions | map(select(.outlier_count > 0))' profile.json

# Outlier summary
jq '.outlier_summary' profile.json

# Slowest single call ever recorded
jq '.timeline_summary.slowest_call' profile.json

# All calls > 1ms in the timeline
jq 'select(.dur_ns > 1000000)' features_demo_timeline.ndjson

# Budget violations only
jq 'select(.budget_exceeded)' features_demo_timeline.ndjson
```
