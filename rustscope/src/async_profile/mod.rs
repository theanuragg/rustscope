//! # Phase 2: Async profiling via tracing-subscriber Layer
//!
//! Integrates with the `tracing` ecosystem to profile async functions
//! correctly — counting only *active poll time*, not suspension time.
//!
//! ## Why this matters for async
//!
//! A naive `#[profile]` guard held across `.await` would record wall time
//! including all suspension time. An `async fn` that sleeps for 100ms would
//! look like it consumed 100ms of CPU. This module hooks into the tracing
//! span lifecycle instead: it only accumulates time while the span is
//! **entered** (a future is being polled), not while it is suspended.
//!
//! ## Metrics per async span
//!
//! | Field | Meaning |
//! |---|---|
//! | `active_time_ns` | Time spent being polled (sum of all poll durations) |
//! | `wall_time_ns` | Total elapsed from span creation to close |
//! | `first_poll_latency_ns` | Time from span creation to first poll (scheduler delay) |
//! | `total_poll_count` | How many times the future was polled |
//! | `total_suspension_count` | How many times the future yielded at `.await` |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rustscope::async_profile;
//! use tracing_subscriber::prelude::*;
//!
//! fn main() {
//!     let _layer = async_profile::install();
//!
//!     tracing_subscriber::registry()
//!         .with(async_profile::layer())
//!         .init();
//!
//!     rustscope::Profiler::init();
//!     // ... run async workload ...
//!     rustscope::Profiler::save_json("profile.json").unwrap();
//! }
//! ```

use std::collections::HashMap;
use std::time::Instant;

use once_cell::sync::{Lazy, OnceCell};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use tracing_core::{
    span::{Attributes, Id, Record},
    Event, Metadata, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use crate::collectors::timing::TimingAccumulator;
use crate::output::schema::{AsyncTaskRecord, TimingStats};

// ─── Output schema ────────────────────────────────────────────────────────────

/// Profile record for one async span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncSpanRecord {
    pub name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub module: Option<String>,

    /// Total times this span was created.
    pub span_count: u64,
    /// Total poll() invocations across all instances.
    pub total_poll_count: u64,
    /// Total .await yield points across all instances.
    pub total_suspension_count: u64,
    /// Max poll count in a single instance (high = long-lived future).
    pub max_polls_per_instance: u64,

    /// Active CPU time per call — only time spent being polled.
    pub active_time_ns: TimingStats,
    /// Wall clock time per call — includes suspension time.
    pub wall_time_ns: TimingStats,
    /// Scheduler latency — span created to first poll.
    pub first_poll_latency_ns: TimingStats,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub task_ids: Vec<u64>,
}

// ─── Per-span runtime state (stored in span extensions) ──────────────────────

struct SpanTimings {
    created_at: Instant,
    first_poll_at: Option<Instant>,
    entered_at: Option<Instant>,
    active_ns_accum: u64,
    poll_count: u64,
    suspension_count: u64,
    task_id: u64,
}

impl SpanTimings {
    fn new() -> Self {
        Self {
            created_at: Instant::now(),
            first_poll_at: None,
            entered_at: None,
            active_ns_accum: 0,
            poll_count: 0,
            suspension_count: 0,
            task_id: next_task_id(),
        }
    }
}

// ─── Per-name aggregate ───────────────────────────────────────────────────────

struct SpanAggregate {
    span_count: u64,
    total_polls: u64,
    total_suspensions: u64,
    max_polls: u64,
    active_timing: TimingAccumulator,
    wall_timing: TimingAccumulator,
    first_poll_timing: TimingAccumulator,
    file: Option<String>,
    line: Option<u32>,
    module: Option<String>,
    task_ids: Vec<u64>,
}

impl SpanAggregate {
    fn new(file: Option<String>, line: Option<u32>, module: Option<String>) -> Self {
        Self {
            span_count: 0, total_polls: 0, total_suspensions: 0, max_polls: 0,
            active_timing: TimingAccumulator::new(),
            wall_timing: TimingAccumulator::new(),
            first_poll_timing: TimingAccumulator::new(),
            file, line, module, task_ids: Vec::new(),
        }
    }

    fn record_closed(&mut self, t: &SpanTimings) {
        let wall_ns = t.created_at.elapsed().as_nanos() as u64;
        self.span_count += 1;
        self.total_polls += t.poll_count;
        self.total_suspensions += t.suspension_count;
        if t.poll_count > self.max_polls { self.max_polls = t.poll_count; }
        self.active_timing.record(t.active_ns_accum, t.active_ns_accum);
        self.wall_timing.record(wall_ns, wall_ns);
        if let Some(fp) = t.first_poll_at {
            let lat = fp.duration_since(t.created_at).as_nanos() as u64;
            self.first_poll_timing.record(lat, lat);
        }
        if self.task_ids.len() < 100 { self.task_ids.push(t.task_id); }
    }

    fn build_record(&self, name: &str, session_ns: u64) -> AsyncSpanRecord {
        AsyncSpanRecord {
            name: name.to_owned(),
            file: self.file.clone(), line: self.line, module: self.module.clone(),
            span_count: self.span_count,
            total_poll_count: self.total_polls,
            total_suspension_count: self.total_suspensions,
            max_polls_per_instance: self.max_polls,
            active_time_ns: self.active_timing.build_stats(session_ns),
            wall_time_ns: self.wall_timing.build_stats(session_ns),
            first_poll_latency_ns: self.first_poll_timing.build_stats(session_ns),
            task_ids: self.task_ids.clone(),
        }
    }
}

// ─── The Layer ────────────────────────────────────────────────────────────────

/// A `tracing_subscriber::Layer` that records per-span async timing.
pub struct RustScopeLayer {
    aggregates: Mutex<HashMap<String, SpanAggregate>>,
    start_time: Instant,
}

impl RustScopeLayer {
    pub fn new() -> Self {
        Self { aggregates: Mutex::new(HashMap::new()), start_time: Instant::now() }
    }

    /// Collect all async span records. Call at session end.
    pub fn collect(&self) -> Vec<AsyncSpanRecord> {
        let session_ns = self.start_time.elapsed().as_nanos() as u64;
        self.aggregates.lock().iter()
            .map(|(name, agg)| agg.build_record(name, session_ns))
            .collect()
    }
}

impl Default for RustScopeLayer { fn default() -> Self { Self::new() } }

impl<S> Layer<S> for RustScopeLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let meta = attrs.metadata();
            let name   = meta.name().to_owned();
            let file   = meta.file().map(|f| f.to_owned());
            let line   = meta.line();
            let module = meta.module_path().map(|m| m.to_owned());
            span.extensions_mut().insert(SpanTimings::new());
            self.aggregates.lock()
                .entry(name)
                .or_insert_with(|| SpanAggregate::new(file, line, module));
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(t) = span.extensions_mut().get_mut::<SpanTimings>() {
                let now = Instant::now();
                if t.first_poll_at.is_none() {
                    t.first_poll_at = Some(now);
                } else {
                    t.suspension_count += 1;
                }
                t.entered_at = Some(now);
                t.poll_count += 1;
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(t) = span.extensions_mut().get_mut::<SpanTimings>() {
                if let Some(entered) = t.entered_at.take() {
                    t.active_ns_accum += entered.elapsed().as_nanos() as u64;
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let name = span.name().to_owned();
            if let Some(t) = span.extensions_mut().remove::<SpanTimings>() {
                if let Some(agg) = self.aggregates.lock().get_mut(&name) {
                    agg.record_closed(&t);
                }
            }
        }
    }

    fn on_record(&self, _: &Id, _: &Record<'_>, _: Context<'_, S>) {}
    fn on_event(&self, _: &Event<'_>, _: Context<'_, S>) {}
}

// ─── Global singleton ─────────────────────────────────────────────────────────

static LAYER: OnceCell<std::sync::Arc<RustScopeLayer>> = OnceCell::new();
static NEXT_TASK_ID: Lazy<std::sync::atomic::AtomicU64> =
    Lazy::new(|| std::sync::atomic::AtomicU64::new(1));

fn next_task_id() -> u64 {
    NEXT_TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Install the global RustScope async layer and return a handle.
/// Must be called before `registry().init()`.
pub fn install() -> std::sync::Arc<RustScopeLayer> {
    let l = std::sync::Arc::new(RustScopeLayer::new());
    let _ = LAYER.set(l.clone());
    l
}

/// Convenience helper for Tokio-based applications.
///
/// Installs a `tracing_subscriber` registry with the RustScope async
/// layer attached. Call this early in `main` before you spawn tasks.
pub fn install_tokio_profiler() {
    use tracing_subscriber::prelude::*;

    let _ = install();
    tracing_subscriber::registry()
        .with(layer())
        .init();
}

/// Returns a Layer impl to pass to `registry().with(...)`.
/// Call `install()` first.
pub fn layer() -> RustScopeLayerShim { RustScopeLayerShim }

pub struct RustScopeLayerShim;

impl<S> Layer<S> for RustScopeLayerShim
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_new_span(attrs, id, ctx); }
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_enter(id, ctx); }
    }
    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_exit(id, ctx); }
    }
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_close(id, ctx); }
    }
    fn on_record(&self, id: &Id, v: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_record(id, v, ctx); }
    }
    fn on_event(&self, e: &Event<'_>, ctx: Context<'_, S>) {
        if let Some(l) = LAYER.get() { l.on_event(e, ctx); }
    }
}

/// Collect async span records from the global layer.
pub fn collect_async_records() -> Vec<AsyncSpanRecord> {
    LAYER.get().map(|l| l.collect()).unwrap_or_default()
}

/// Snapshot async tasks into high-level AsyncTaskRecord entries.
pub fn collect_async_tasks() -> Vec<AsyncTaskRecord> {
    let spans = collect_async_records();
    spans
        .into_iter()
        .map(|s| AsyncTaskRecord {
            task_id: s
                .task_ids
                .get(0)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            name: s.name.clone(),
            polls: s.total_poll_count,
            wakeups: s.total_suspension_count,
            total_runtime_ns: s.active_time_ns.total_ns,
            suspension_time_ns: s.wall_time_ns.total_ns.saturating_sub(s.active_time_ns.total_ns),
        })
        .collect()
}
