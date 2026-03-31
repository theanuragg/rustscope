"use client";

import React, { useState } from "react";
import type { ProfileData } from "@/types/profiler";
import { formatNs, formatPct } from "@/lib/parse-profile";

interface Props {
  data: ProfileData;
  onSelectZone: (zone: any) => void;
}

interface TreeNodeProps {
  node: any;
  depth: number;
  onSelectZone: (zone: any) => void;
  functionColors: Record<string, string>;
}

function TreeNode({ node, depth, onSelectZone, functionColors }: TreeNodeProps) {
  const [isOpen, setIsOpen] = useState(true);
  const color = functionColors[node.name] || "var(--zone-gray)";

  return (
    <div className="flex flex-col">
      <div
        onClick={() => {
          setIsOpen(!isOpen);
          onSelectZone(node);
        }}
        className={`flex items-center h-[20px] cursor-pointer hover:bg-[var(--bg3)] px-1 group text-[11px] border-b border-[var(--border)]`}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
      >
        <span className="w-3 text-[var(--text2)] mr-1">
          {node.children && node.children.length > 0 ? (isOpen ? "▾" : "▶") : ""}
        </span>
        <span
          className="w-3 h-3 rounded-sm mr-2 flex-shrink-0"
          style={{ backgroundColor: color }}
        />
        <span className="truncate flex-1 text-[var(--text0)]" title={node.name}>
          {node.name}
        </span>
        <span className="text-[var(--text1)] px-2">{formatNs(node.duration_ns || 0)}</span>
        <span className="text-[var(--text2)] w-10 text-right">
          {formatPct((node.duration_ns / 1_000_000_000) * 100)}
        </span>
      </div>
      {isOpen && node.children && node.children.map((child: any, i: number) => (
        <TreeNode
          key={i}
          node={child}
          depth={depth + 1}
          onSelectZone={onSelectZone}
          functionColors={functionColors}
        />
      ))}
    </div>
  );
}

export function ZonesTab({ data, onSelectZone }: Props) {
  const functionColors = React.useMemo(() => {
    const colors = [
      "var(--zone-red)", "var(--zone-orange)", "var(--zone-amber)", 
      "var(--zone-green)", "var(--zone-teal)", "var(--zone-blue)", 
      "var(--zone-purple)", "var(--zone-pink)", "var(--zone-gray)"
    ];
    const map: Record<string, string> = {};
    data.functions.forEach((f, i) => {
      map[f.name] = colors[i % colors.length];
    });
    return map;
  }, [data.functions]);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex-1 overflow-auto bg-[var(--bg0)]">
        {data.raw?.call_trees?.map((root: any, i: number) => (
          <TreeNode
            key={i}
            node={root}
            depth={0}
            onSelectZone={onSelectZone}
            functionColors={functionColors}
          />
        ))}
      </div>

      {/* Zone Color Legend */}
      <div className="h-[120px] border-t border-[var(--border2)] bg-[var(--bg1)] p-2 shrink-0 overflow-auto">
        <div className="tracy-label-caps mb-1 uppercase">zone color legend</div>
        <div className="grid grid-cols-2 gap-x-2 gap-y-1">
          {data.functions.map((f, i) => (
            <div key={f.name} className="flex items-center gap-2 text-[10px] h-[16px] truncate">
              <div
                className="w-3 h-3 rounded-sm flex-shrink-0"
                style={{ backgroundColor: functionColors[f.name] }}
              />
              <span className="truncate text-[var(--text1)]" title={f.name}>{f.name}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
