"use client";

import React from "react";
import type { ProfileFrame, ProfilerAction } from "@/types/profiler";
import clsx from "clsx";

interface Props {
  zoomStack: ProfileFrame[];
  isIcicle: boolean;
  dispatch: React.Dispatch<ProfilerAction>;
}

export function Breadcrumb({ zoomStack, isIcicle, dispatch }: Props) {
  if (zoomStack.length === 0) return null;

  const accentCls = isIcicle ? "text-blue-600" : "text-orange-600";

  return (
    <div className="flex items-center gap-1.5 px-3 py-2 bg-stone-50 border-t border-stone-100 font-mono text-[11px] overflow-x-auto">
      <button
        onClick={() => dispatch({ type: "ZOOM_RESET" })}
        className="text-stone-400 hover:text-stone-600 whitespace-nowrap"
      >
        root
      </button>
      {zoomStack.map((f, i) => (
        <React.Fragment key={i}>
          <span className="text-stone-300">›</span>
          <button
            onClick={() => dispatch({ type: "ZOOM_TO", depth: i })}
            className={clsx(
              "whitespace-nowrap truncate max-w-[180px]",
              i === zoomStack.length - 1
                ? `${accentCls} font-medium cursor-default`
                : "text-stone-500 hover:text-stone-700"
            )}
          >
            {f.displayName}
          </button>
        </React.Fragment>
      ))}
    </div>
  );
}
