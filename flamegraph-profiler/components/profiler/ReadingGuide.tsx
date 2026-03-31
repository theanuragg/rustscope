"use client";

import React, { useState } from "react";
import type { ProfileMode, ChartType } from "@/types/profiler";
import clsx from "clsx";

interface Props {
  mode: ProfileMode;
  chartType: ChartType;
}

const GUIDE_CONTENT: Record<ProfileMode, {
  title: string;
  what: string;
  width: string;
  height: string;
  tip: string;
}> = {
  cpu: {
    title: "reading a cpu flame graph",
    what: "Each bar is a function on the call stack. The profiler interrupted your process ~1000×/sec and recorded the full call stack each time.",
    width: "Width = % of CPU samples that included this function. Wider = more CPU consumed. The widest bars are your bottlenecks.",
    height: "Height = call depth. The bottom bar is your process root. Each row above it is a function called by the one below.",
    tip: "Look for wide bars near the top of a tall stack — that's a leaf function burning real CPU with no subcalls to hide in.",
  },
  alloc: {
    title: "reading an allocation profile",
    what: "Each bar shows a function's share of heap allocations. This is sampled at alloc sites — not CPU time.",
    width: "Width = % of total heap allocations that passed through this function. Wider = allocates more.",
    height: "Height = call depth into the allocating call stack. Find the true allocation site at the top of a tall stack.",
    tip: "If Vec::push or String::from appear wide, you have unbounded collection growth. If __rdl_alloc is wide, your allocator is thrashing.",
  },
  offcpu: {
    title: "reading an off-cpu profile",
    what: "This shows time your process was blocked — NOT executing. CPU profiles are blind to this. Off-CPU reveals I/O waits, mutex contention, and scheduler latency.",
    width: "Width = % of total wall-clock blocked time. Wider = more time your process was asleep waiting.",
    height: "Height = depth of the blocked call stack. The blocking syscall (epoll_wait, futex) is usually near the top.",
    tip: "If epoll_wait is the widest frame, you're I/O bound. If futex_wait is wide, you have lock contention. These require different fixes.",
  },
};

export function ReadingGuide({ mode, chartType }: Props) {
  const [open, setOpen] = useState(false);
  const content = GUIDE_CONTENT[mode];
  const isIcicle = chartType === "icicle";

  return (
    <div
      className={clsx(
        "border rounded-xl overflow-hidden transition-colors",
        isIcicle ? "border-blue-100 bg-blue-50/30" : "border-orange-100 bg-orange-50/20"
      )}
    >
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center justify-between px-4 py-2.5 text-left"
      >
        <div className="flex items-center gap-2">
          <span
            className={clsx(
              "w-4 h-4 rounded-full flex items-center justify-center text-[10px] font-bold text-white flex-shrink-0",
              isIcicle ? "bg-blue-500" : "bg-orange-500"
            )}
          >
            ?
          </span>
          <span className="font-mono text-[11px] text-stone-500 uppercase tracking-widest">
            {content.title}
          </span>
        </div>
        <span className="text-stone-400 text-[11px] font-mono">
          {open ? "hide" : "show"}
        </span>
      </button>

      {open && (
        <div className="px-4 pb-4 grid grid-cols-1 sm:grid-cols-3 gap-3 border-t border-stone-100">
          <GuideCard icon="📡" heading="what you&apos;re seeing" body={content.what} />
          <GuideCard icon="↔" heading="what width means" body={content.width} />
          <GuideCard icon="↕" heading="what height means" body={content.height} />
          <div className="sm:col-span-3">
            <div
              className={clsx(
                "flex items-start gap-2 px-3 py-2.5 rounded-lg border",
                isIcicle ? "bg-blue-50 border-blue-200" : "bg-amber-50 border-amber-200"
              )}
            >
              <span className="text-sm">💡</span>
              <p className="font-sans text-[12px] text-stone-600 leading-relaxed">
                {content.tip}
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function GuideCard({ icon, heading, body }: { icon: string; heading: string; body: string }) {
  return (
    <div className="bg-white border border-stone-100 rounded-lg px-3 py-2.5 mt-3">
      <p className="font-sans text-[11px] font-semibold text-stone-700 mb-1">
        {icon} {heading}
      </p>
      <p className="font-sans text-[11px] text-stone-500 leading-relaxed">{body}</p>
    </div>
  );
}
