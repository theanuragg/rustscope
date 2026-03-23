//! Stack frame size estimation.
//!
//! ## How it works
//!
//! We read the current stack pointer (SP) at function entry and at return,
//! then compute the difference. This gives the total stack consumed between
//! the two points, which approximates the callee's frame size (+ spill area).
//!
//! ## Limitations
//! - This is an *approximation*. Inlining, tail-call optimisation, and LLVM
//!   stack coloring all affect the actual layout.
//! - Only meaningful in debug/profiling builds where frames aren't elided.
//! - x86_64, aarch64 only (falls back to `None` elsewhere).
//! - Should only be used in non-recursive contexts; for recursive functions,
//!   the value reflects the topmost frame only.

/// Read the current stack pointer.
///
/// Returns `None` on unsupported architectures.
#[inline(always)]
pub fn read_sp() -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        let sp: usize;
        unsafe {
            std::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack, preserves_flags));
        }
        Some(sp)
    }
    #[cfg(target_arch = "aarch64")]
    {
        let sp: usize;
        unsafe {
            std::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags));
        }
        Some(sp)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        None
    }
}

/// Compute the stack consumed between `sp_at_entry` and `sp_now`.
///
/// Stacks grow downward, so entry SP ≥ current SP.
#[inline]
pub fn frame_size(sp_at_entry: usize, sp_now: usize) -> u64 {
    sp_at_entry.saturating_sub(sp_now) as u64
}

/// Track per-thread call depth (for recursion detection).
pub fn current_call_depth() -> u32 {
    DEPTH.with(|d| *d.borrow())
}

pub fn push_depth() -> u32 {
    DEPTH.with(|d| {
        let mut b = d.borrow_mut();
        *b += 1;
        *b
    })
}

pub fn pop_depth() {
    DEPTH.with(|d| {
        let mut b = d.borrow_mut();
        if *b > 0 { *b -= 1; }
    });
}

thread_local! {
    static DEPTH: std::cell::RefCell<u32> = std::cell::RefCell::new(0);
}
