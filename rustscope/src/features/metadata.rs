//! # Session metadata
//!
//! Attach arbitrary key-value metadata to a profiling session.
//! Useful for correlating profiles with CI runs, git commits, config variants.
//!
//! ## Usage
//!
//! ```rust
//! use rustscope::metadata;
//!
//! fn main() {
//!     rustscope::Profiler::init();
//!     
//!     // Attach metadata before running code
//!     metadata::set("git_commit", "abc123");
//!     metadata::set("branch", "main");
//!     metadata::set("config", "release-lto");
//!     metadata::set_u64("build_number", 4821);
//!     metadata::tag("nightly");
//!     metadata::tag("regression-test");
//!
//!     // All metadata appears in profile.json under `session_meta`
//!     rustscope::Profiler::save_json("profile.json").unwrap();
//! }
//! ```
//!
//! ## In the JSON
//!
//! ```json
//! {
//!   "session_meta": {
//!     "git_commit": "abc123",
//!     "branch": "main",
//!     "config": "release-lto",
//!     "build_number": "4821",
//!     "tags": ["nightly", "regression-test"],
//!     "session_name": "my_benchmark_run"
//!   }
//! }
//! ```

use std::collections::HashMap;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

// ─── schema ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMeta {
    /// Arbitrary string key-value pairs.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub kv: HashMap<String, String>,
    /// Set of string tags (unordered labels without values).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Optional human-readable name for this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    /// Description of what this session is profiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional environment label ("ci", "dev", "staging", "prod").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

// ─── global state ─────────────────────────────────────────────────────────────

static META: Lazy<RwLock<SessionMeta>> = Lazy::new(|| RwLock::new(SessionMeta::default()));

// ─── public API ──────────────────────────────────────────────────────────────

/// Set a string key-value metadata pair.
pub fn set(key: &str, value: &str) {
    META.write().kv.insert(key.to_owned(), value.to_owned());
}

/// Set a numeric value (stored as string to keep schema simple).
pub fn set_u64(key: &str, value: u64) {
    META.write().kv.insert(key.to_owned(), value.to_string());
}

/// Set a float value.
pub fn set_f64(key: &str, value: f64) {
    META.write().kv.insert(key.to_owned(), format!("{:.6}", value));
}

/// Add a tag (idempotent — no duplicates added).
pub fn tag(label: &str) {
    let mut m = META.write();
    if !m.tags.contains(&label.to_owned()) {
        m.tags.push(label.to_owned());
    }
}

/// Remove a tag.
pub fn untag(label: &str) {
    META.write().tags.retain(|t| t != label);
}

/// Set a human-readable name for this session.
pub fn set_name(name: &str) {
    META.write().session_name = Some(name.to_owned());
}

/// Set a description.
pub fn set_description(desc: &str) {
    META.write().description = Some(desc.to_owned());
}

/// Set the environment label.
pub fn set_environment(env: &str) {
    META.write().environment = Some(env.to_owned());
}

/// Bulk-set from environment variables with a prefix.
///
/// Reads all env vars matching `prefix_*` and adds them as metadata.
/// E.g., with prefix `"RUSTSCOPE"`, reads `RUSTSCOPE_GIT_COMMIT`,
/// `RUSTSCOPE_BUILD_NUMBER`, etc.
pub fn load_from_env(prefix: &str) {
    let prefix_upper = format!("{}_", prefix.to_uppercase());
    let mut m = META.write();
    for (key, value) in std::env::vars() {
        if key.starts_with(&prefix_upper) {
            let short_key = key[prefix_upper.len()..].to_lowercase();
            m.kv.insert(short_key, value);
        }
    }
}

/// Capture common CI metadata automatically (GitHub Actions, GitLab CI, etc.).
pub fn auto_detect_ci() {
    // GitHub Actions
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        tag("github-actions");
        if let Ok(v) = std::env::var("GITHUB_SHA") { set("git_commit", &v[..8.min(v.len())]); }
        if let Ok(v) = std::env::var("GITHUB_REF_NAME") { set("branch", &v); }
        if let Ok(v) = std::env::var("GITHUB_RUN_NUMBER") { set("ci_run", &v); }
        if let Ok(v) = std::env::var("GITHUB_REPOSITORY") { set("repo", &v); }
        set("environment", "ci");
    }
    // GitLab CI
    if std::env::var("GITLAB_CI").is_ok() {
        tag("gitlab-ci");
        if let Ok(v) = std::env::var("CI_COMMIT_SHORT_SHA") { set("git_commit", &v); }
        if let Ok(v) = std::env::var("CI_COMMIT_REF_NAME") { set("branch", &v); }
        if let Ok(v) = std::env::var("CI_PIPELINE_ID") { set("ci_run", &v); }
        set("environment", "ci");
    }
    // Generic CI detection
    if std::env::var("CI").is_ok() {
        tag("ci");
        set("environment", "ci");
    }

    // Capture git commit from git command if not already set
    let m = META.read();
    let has_commit = m.kv.contains_key("git_commit");
    drop(m);
    if !has_commit {
        if let Ok(out) = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
        {
            if out.status.success() {
                let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
                set("git_commit", &sha);
            }
        }
    }
}

/// Get the current metadata snapshot.
pub fn get() -> SessionMeta {
    META.read().clone()
}

/// Check if metadata has any content (used to decide whether to include in JSON).
pub fn is_empty() -> bool {
    let m = META.read();
    m.kv.is_empty() && m.tags.is_empty() && m.session_name.is_none()
}

/// Clear all metadata.
pub fn reset() {
    *META.write() = SessionMeta::default();
}
