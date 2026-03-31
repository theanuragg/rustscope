import type { ProfileFrame, ProfileMode, ProfileMeta, Insight } from "@/types/profiler";
import { formatNs, formatBytes } from "./demangle";

export function generateInsights(
  frames: ProfileFrame[],
  mode: ProfileMode,
  meta: ProfileMeta
): Insight[] {
  const insights: Insight[] = [];

  // Sort by total time descending
  const byTotal = [...frames].sort((a, b) => b.totalPct - a.totalPct);
  const bySelf  = [...frames].sort((a, b) => b.selfPct  - a.selfPct);

  const top = byTotal[0];
  const topSelf = bySelf[0];

  if (!top) return [];

  if (mode === "cpu") {
    // 1. Hottest total frame
    if (top.totalPct > 25) {
      insights.push({
        severity: "critical",
        title: `${top.displayName} consumes ${top.totalPct.toFixed(1)}% of CPU`,
        body: `This is the single hottest call tree. Everything it calls is costing you ${formatNs(top.totalNs)} total. Zoom in to find the true self-time offender.`,
        relatedFrame: top.name,
      });
    }

    // 2. High self-time = genuine hot spot
    if (topSelf.selfPct > 10 && topSelf !== top) {
      insights.push({
        severity: topSelf.selfPct > 20 ? "critical" : "warn",
        title: `${topSelf.displayName} has ${topSelf.selfPct.toFixed(1)}% self-time`,
        body: `High self-time means this function itself (not its callees) is expensive. This is a genuine hot loop — consider optimising the algorithm or adding SIMD.`,
        relatedFrame: topSelf.name,
      });
    }

    // 3. Async runtime overhead
    const runtimeFrames = frames.filter((f) => f.layer === "runtime");
    const runtimePct = runtimeFrames.reduce((s, f) => s + f.selfPct, 0);
    if (runtimePct > 15) {
      insights.push({
        severity: "warn",
        title: `Async runtime overhead is ${runtimePct.toFixed(1)}%`,
        body: `Tokio/runtime frames are consuming significant CPU. This often means too many short tasks, task scheduling contention, or excessive Waker clones. Consider batching work or using blocking thread pools for CPU-heavy futures.`,
      });
    }

    // 4. Allocator pressure in CPU profile
    const allocFrames = frames.filter((f) => f.layer === "alloc");
    const allocPct = allocFrames.reduce((s, f) => s + f.selfPct, 0);
    if (allocPct > 5) {
      insights.push({
        severity: "warn",
        title: `Allocator is ${allocPct.toFixed(1)}% of CPU samples`,
        body: `Heap allocation is showing up in CPU profiles — meaning your hot path allocates frequently. Switch to arena allocation, pre-allocate buffers, or use stack-pinned futures.`,
      });
    }

    // 5. Kernel time
    const kernelFrames = frames.filter((f) => f.layer === "kernel");
    const kernelPct = kernelFrames.reduce((s, f) => s + f.totalPct, 0);
    if (kernelPct > 20) {
      insights.push({
        severity: "warn",
        title: `${kernelPct.toFixed(0)}% of time in kernel`,
        body: `High kernel time suggests excessive syscalls — likely from many small I/O operations, frequent mmap/munmap (allocator churn), or signal delivery. Batch syscalls where possible.`,
      });
    }
  }

  if (mode === "alloc") {
    if (top.totalPct > 20) {
      insights.push({
        severity: "critical",
        title: `${top.displayName} responsible for ${top.totalPct.toFixed(1)}% of allocations`,
        body: `This is your top allocation site. Every call here creates heap objects. If it's in a hot loop, you're paying O(n) allocator overhead. Consider pre-allocation or object pooling.`,
        relatedFrame: top.name,
      });
    }

    if (meta.peakHeapBytes) {
      insights.push({
        severity: "info",
        title: `Peak heap: ${formatBytes(meta.peakHeapBytes)}`,
        body: `This is the maximum live heap size observed. If this exceeds your RSS budget, look for retained collections or unbounded caches.`,
      });
    }

    // Arc clone hotspots
    const arcHeavy = frames.filter((f) => (f.arcClones ?? 0) > 1000);
    if (arcHeavy.length > 0) {
      insights.push({
        severity: "warn",
        title: `Heavy Arc cloning detected in ${arcHeavy.length} frame(s)`,
        body: `Frequent Arc::clone causes atomic refcount increments which are cache-line-contended on multi-core. Consider using indices, Rc for single-threaded paths, or restructuring ownership.`,
      });
    }
  }

  if (mode === "offcpu") {
    if (top.totalPct > 30) {
      insights.push({
        severity: "critical",
        title: `${top.displayName} blocks ${top.totalPct.toFixed(1)}% of wall time`,
        body: `This is where your process spends the most wall-clock time waiting. This is invisible to CPU profilers — fix this first before optimising CPU.`,
        relatedFrame: top.name,
      });
    }

    const asyncFrames = frames.filter((f) => f.isAsync);
    const asyncPct = asyncFrames.reduce((s, f) => s + f.totalPct, 0);
    if (asyncPct > 20) {
      insights.push({
        severity: "warn",
        title: `${asyncPct.toFixed(0)}% of blocked time is in async futures`,
        body: `Async futures are spending significant time pending. This may indicate I/O bound operations that could benefit from connection pooling, prefetching, or reducing round-trips.`,
      });
    }
  }

  // Always: crate coverage summary
  const crateFrames = frames.filter((f) => f.layer === "crate");
  const cratePct = crateFrames.reduce((s, f) => s + f.selfPct, 0);
  if (cratePct < 10 && mode === "cpu") {
    insights.push({
      severity: "info",
      title: `Only ${cratePct.toFixed(1)}% self-time in your crate`,
      body: `Most CPU time is in dependencies or the runtime. Your code itself is not the bottleneck — focus on which library calls you make, not how they're implemented.`,
    });
  }

  return insights;
}
