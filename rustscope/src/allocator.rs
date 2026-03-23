//! Re-exports the `TrackingAllocator` and memory snapshot types.
//!
//! ## Usage
//!
//! In your binary crate's `main.rs`:
//!
//! ```rust
//! use rustscope::allocator::TrackingAllocator;
//!
//! #[global_allocator]
//! static ALLOC: TrackingAllocator = TrackingAllocator;
//!
//! fn main() {
//!     rustscope::Profiler::init();
//!     // ... your code
//!     rustscope::Profiler::save_json("profile.json").unwrap();
//! }
//! ```
//!
//! The allocator wraps the system allocator with zero-overhead atomic counters.
//! There is no lock involved in the hot path.

pub use crate::collectors::memory::{TrackingAllocator, AllocSnapshot, try_snapshot, snapshot};
