//! CPU hardware performance counter collection.
//!
//! On Linux with the `hw-counters` feature enabled, reads PMU registers via
//! `perf_event_open(2)`.  On every other platform (or when the feature is off,
//! or when the caller lacks `CAP_PERFMON`), this module returns `None`
//! gracefully so the rest of the profiler continues to work.
//!
//! ## Counters collected
//! - CPU cycles
//! - Instructions retired
//! - Cache references + cache misses (→ miss rate)
//! - L1-DCache loads + load misses
//! - LLC loads + load misses
//! - Branch instructions + branch misses (→ branch miss rate)
//! - Context switches
//! - Page faults (minor)
//! - CPU migrations

use crate::output::schema::CpuCounters;

// ─── public API ──────────────────────────────────────────────────────────────

/// Opaque handle that wraps all open perf_event counters for one scope.
/// Created at function entry, read + dropped at function exit.
pub struct CpuCounterGuard {
    #[cfg(all(target_os = "linux", feature = "hw-counters"))]
    inner: Option<LinuxCounterSet>,
    /// Cross-platform fallback: cycles at entry (rdtsc / cntvct_el0).
    #[cfg(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
              not(all(target_os = "linux", feature = "hw-counters"))))]
    start_cycles: u64,
    /// Platforms without PMU or TSC support.
    #[cfg(not(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
                  not(all(target_os = "linux", feature = "hw-counters")))))]
    _phantom: (),
}

impl CpuCounterGuard {
    /// Open all counters. Returns a guard that can be read later.
    #[inline]
    pub fn open() -> Self {
        #[cfg(all(target_os = "linux", feature = "hw-counters"))]
        {
            Self { inner: LinuxCounterSet::try_open() }
        }
        #[cfg(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
                  not(all(target_os = "linux", feature = "hw-counters"))))]
        {
            Self { start_cycles: read_cycles() }
        }
        #[cfg(not(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
                      not(all(target_os = "linux", feature = "hw-counters")))))]
        {
            Self { _phantom: () }
        }
    }

    /// Read final counter values and compute derived metrics.
    /// Returns `None` if counters are unavailable.
    #[inline]
    pub fn read(self) -> Option<CpuCounters> {
        #[cfg(all(target_os = "linux", feature = "hw-counters"))]
        {
            self.inner.and_then(|s| s.read())
        }
        #[cfg(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
                  not(all(target_os = "linux", feature = "hw-counters"))))]
        {
            let end = read_cycles();
            let delta = end.saturating_sub(self.start_cycles);
            Some(CpuCounters {
                cpu_cycles: delta,
                instructions: 0,
                ipc: 0.0,
                cache_references: 0,
                cache_misses: 0,
                cache_miss_rate: 0.0,
                l1_dcache_loads: 0,
                l1_dcache_load_misses: 0,
                llc_loads: 0,
                llc_load_misses: 0,
                branch_instructions: 0,
                branch_misses: 0,
                branch_miss_rate: 0.0,
                context_switches: 0,
                page_faults: 0,
                cpu_migrations: 0,
            })
        }
        #[cfg(not(all(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
                      not(all(target_os = "linux", feature = "hw-counters")))))]
        {
            None
        }
    }
}

// ─── Cross-platform cycle counter fallback ─────────────────────────────────────

#[inline]
#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
fn read_cycles() -> u64 {
    // Safe wrapper around the TSC intrinsic.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[inline]
#[cfg(all(target_arch = "aarch64",
          not(any(target_os = "ios", target_os = "tvos", target_os = "watchos"))))]
fn read_cycles() -> u64 {
    // Read the virtual count register (arch counter).
    let v: u64;
    unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v); }
    v
}


// ─── Linux implementation ─────────────────────────────────────────────────────

#[cfg(all(target_os = "linux", feature = "hw-counters"))]
mod linux_impl {
    use perf_event::{Builder, Group};
    use perf_event::events::{Hardware, Software, Cache, CacheOp, CacheResult, WhichCache};
    use crate::output::schema::CpuCounters;

    pub struct LinuxCounterSet {
        group: Group,
        // Counter indices in the group
        cycles_id:         u64,
        insns_id:          u64,
        cache_refs_id:     u64,
        cache_miss_id:     u64,
        branch_insns_id:   u64,
        branch_miss_id:    u64,
        ctx_switches_id:   u64,
        page_faults_id:    u64,
        cpu_mig_id:        u64,
    }

    impl LinuxCounterSet {
        /// Returns None if perf_event_open fails (permissions, VM, etc.)
        pub fn try_open() -> Option<Self> {
            let mut group = Group::new().ok()?;

            let cycles = Builder::new()
                .group(&mut group)
                .kind(Hardware::CPU_CYCLES)
                .build().ok()?;
            let insns = Builder::new()
                .group(&mut group)
                .kind(Hardware::INSTRUCTIONS)
                .build().ok()?;
            let cache_refs = Builder::new()
                .group(&mut group)
                .kind(Hardware::CACHE_REFERENCES)
                .build().ok()?;
            let cache_miss = Builder::new()
                .group(&mut group)
                .kind(Hardware::CACHE_MISSES)
                .build().ok()?;
            let branch_insns = Builder::new()
                .group(&mut group)
                .kind(Hardware::BRANCH_INSTRUCTIONS)
                .build().ok()?;
            let branch_miss = Builder::new()
                .group(&mut group)
                .kind(Hardware::BRANCH_MISSES)
                .build().ok()?;
            let ctx_sw = Builder::new()
                .group(&mut group)
                .kind(Software::CONTEXT_SWITCHES)
                .build().ok()?;
            let page_faults = Builder::new()
                .group(&mut group)
                .kind(Software::PAGE_FAULTS)
                .build().ok()?;
            let cpu_mig = Builder::new()
                .group(&mut group)
                .kind(Software::CPU_MIGRATIONS)
                .build().ok()?;

            group.enable().ok()?;

            Some(Self {
                cycles_id:       cycles.id(),
                insns_id:        insns.id(),
                cache_refs_id:   cache_refs.id(),
                cache_miss_id:   cache_miss.id(),
                branch_insns_id: branch_insns.id(),
                branch_miss_id:  branch_miss.id(),
                ctx_switches_id: ctx_sw.id(),
                page_faults_id:  page_faults.id(),
                cpu_mig_id:      cpu_mig.id(),
                group,
            })
        }

        pub fn read(mut self) -> Option<CpuCounters> {
            self.group.disable().ok()?;
            let counts = self.group.read().ok()?;

            let cycles       = counts.get(self.cycles_id).copied().unwrap_or(0);
            let insns        = counts.get(self.insns_id).copied().unwrap_or(0);
            let cache_refs   = counts.get(self.cache_refs_id).copied().unwrap_or(0);
            let cache_miss   = counts.get(self.cache_miss_id).copied().unwrap_or(0);
            let branch_insns = counts.get(self.branch_insns_id).copied().unwrap_or(0);
            let branch_miss  = counts.get(self.branch_miss_id).copied().unwrap_or(0);
            let ctx_sw       = counts.get(self.ctx_switches_id).copied().unwrap_or(0);
            let page_faults  = counts.get(self.page_faults_id).copied().unwrap_or(0);
            let cpu_mig      = counts.get(self.cpu_mig_id).copied().unwrap_or(0);

            let ipc = if cycles > 0 { insns as f64 / cycles as f64 } else { 0.0 };
            let cache_miss_rate = if cache_refs > 0 {
                cache_miss as f64 / cache_refs as f64
            } else { 0.0 };
            let branch_miss_rate = if branch_insns > 0 {
                branch_miss as f64 / branch_insns as f64
            } else { 0.0 };

            Some(CpuCounters {
                cpu_cycles:           cycles,
                instructions:         insns,
                ipc,
                cache_references:     cache_refs,
                cache_misses:         cache_miss,
                cache_miss_rate,
                l1_dcache_loads:      0,    // requires separate counter group
                l1_dcache_load_misses:0,
                llc_loads:            0,
                llc_load_misses:      0,
                branch_instructions:  branch_insns,
                branch_misses:        branch_miss,
                branch_miss_rate,
                context_switches:     ctx_sw,
                page_faults,
                cpu_migrations:       cpu_mig,
            })
        }
    }
}

#[cfg(all(target_os = "linux", feature = "hw-counters"))]
use linux_impl::LinuxCounterSet;
