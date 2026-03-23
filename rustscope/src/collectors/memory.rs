//! Heap memory tracking via a custom global allocator wrapper.
//!
//! ## Usage
//!
//! In your binary (not a library crate):
//!
//! ```rust
//! use rustscope::allocator::TrackingAllocator;
//!
//! #[global_allocator]
//! static ALLOC: TrackingAllocator = TrackingAllocator;
//! ```
//!
//! If the tracking allocator is NOT installed, all memory metrics will be
//! `None` / 0 in the JSON output — the profiler will still work.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};

// ─── global counters ─────────────────────────────────────────────────────────

/// Current net heap bytes (can temporarily go negative during realloc races).
pub static CURRENT_HEAP: AtomicI64 = AtomicI64::new(0);
/// Highest value CURRENT_HEAP has ever reached.
pub static PEAK_HEAP: AtomicU64 = AtomicU64::new(0);
/// Total bytes handed out by alloc() + realloc() growth.
pub static TOTAL_ALLOC: AtomicU64 = AtomicU64::new(0);
/// Total bytes returned to dealloc() + realloc() shrink.
pub static TOTAL_DEALLOC: AtomicU64 = AtomicU64::new(0);
/// Number of allocator operations (alloc calls + realloc calls).
pub static ALLOC_OPS: AtomicU64 = AtomicU64::new(0);
/// Set to true once the allocator has been used at least once.
pub static ALLOCATOR_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// ─── allocator ───────────────────────────────────────────────────────────────

/// A drop-in replacement for the system allocator that records heap stats.
///
/// Thread-safe; uses relaxed atomics (no cross-thread ordering guarantee,
/// but individual counters are correct).
pub struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let size = layout.size() as i64;
            let cur = CURRENT_HEAP.fetch_add(size, Relaxed) + size;
            TOTAL_ALLOC.fetch_add(size as u64, Relaxed);
            ALLOC_OPS.fetch_add(1, Relaxed);
            ALLOCATOR_ACTIVE.store(true, Relaxed);
            update_peak(cur.max(0) as u64);
        }
        ptr
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        CURRENT_HEAP.fetch_sub(layout.size() as i64, Relaxed);
        TOTAL_DEALLOC.fetch_add(layout.size() as u64, Relaxed);
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            let diff = new_size as i64 - layout.size() as i64;
            let cur = CURRENT_HEAP.fetch_add(diff, Relaxed) + diff;
            if diff > 0 {
                TOTAL_ALLOC.fetch_add(diff as u64, Relaxed);
                ALLOC_OPS.fetch_add(1, Relaxed);
                update_peak(cur.max(0) as u64);
            } else {
                TOTAL_DEALLOC.fetch_add((-diff) as u64, Relaxed);
            }
        }
        new_ptr
    }
}

#[inline]
fn update_peak(cur: u64) {
    let mut peak = PEAK_HEAP.load(Relaxed);
    while cur > peak {
        match PEAK_HEAP.compare_exchange_weak(peak, cur, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(p) => peak = p,
        }
    }
}

// ─── snapshot ────────────────────────────────────────────────────────────────

/// A point-in-time snapshot of the allocator counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocSnapshot {
    pub current: i64,
    pub peak: u64,
    pub total_alloc: u64,
    pub total_dealloc: u64,
    pub alloc_ops: u64,
}

/// Returns `None` if `TrackingAllocator` has never been used.
pub fn try_snapshot() -> Option<AllocSnapshot> {
    if !ALLOCATOR_ACTIVE.load(Relaxed) {
        return None;
    }
    Some(AllocSnapshot {
        current:      CURRENT_HEAP.load(Relaxed),
        peak:         PEAK_HEAP.load(Relaxed),
        total_alloc:  TOTAL_ALLOC.load(Relaxed),
        total_dealloc:TOTAL_DEALLOC.load(Relaxed),
        alloc_ops:    ALLOC_OPS.load(Relaxed),
    })
}

/// Always returns a snapshot (zeros if allocator not active).
pub fn snapshot() -> AllocSnapshot {
    AllocSnapshot {
        current:      CURRENT_HEAP.load(Relaxed),
        peak:         PEAK_HEAP.load(Relaxed),
        total_alloc:  TOTAL_ALLOC.load(Relaxed),
        total_dealloc:TOTAL_DEALLOC.load(Relaxed),
        alloc_ops:    ALLOC_OPS.load(Relaxed),
    }
}

impl AllocSnapshot {
    /// Bytes allocated between `earlier` and `self`.
    #[inline]
    pub fn alloc_delta(&self, earlier: &AllocSnapshot) -> u64 {
        self.total_alloc.saturating_sub(earlier.total_alloc)
    }
    /// Bytes freed between `earlier` and `self`.
    #[inline]
    pub fn dealloc_delta(&self, earlier: &AllocSnapshot) -> u64 {
        self.total_dealloc.saturating_sub(earlier.total_dealloc)
    }
    /// Allocator ops between `earlier` and `self`.
    #[inline]
    pub fn ops_delta(&self, earlier: &AllocSnapshot) -> u64 {
        self.alloc_ops.saturating_sub(earlier.alloc_ops)
    }
    /// Peak heap observed between `earlier` and `self`.
    #[inline]
    pub fn peak_delta(&self, earlier: &AllocSnapshot) -> u64 {
        // Peak is global and monotonically increasing — take max above entry level.
        self.peak.saturating_sub(earlier.peak.max(earlier.current.max(0) as u64))
    }
}
