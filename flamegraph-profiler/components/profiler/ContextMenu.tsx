"use client";

import React from "react";

interface Props {
  x: number;
  y: number;
  onClose: () => void;
}

export function ContextMenu({ x, y, onClose }: Props) {
  const items = [
    { label: "View statistics", onClick: () => {} },
    { label: "Copy function name", onClick: () => {} },
    { label: "Add annotation...", onClick: () => {} },
    { label: "Set as filter", onClick: () => {} },
    { type: "separator" },
    { label: "Zoom to this zone", onClick: () => {} },
    { label: "Jump to source", onClick: () => {} },
  ];

  return (
    <>
      <div
        className="fixed inset-0 z-[100]"
        onClick={onClose}
        onContextMenu={(e) => {
          e.preventDefault();
          onClose();
        }}
      />
      <div
        style={{ left: x, top: y }}
        className="fixed bg-[var(--bg0)] border border-[var(--border2)] z-[101] shadow-none min-w-[160px] py-1 select-none"
      >
        {items.map((item, i) => {
          if (item.type === "separator") {
            return <div key={i} className="h-[1px] bg-[var(--border2)] my-1" />;
          }
          return (
            <div
              key={i}
              onClick={() => {
                item.onClick?.();
                onClose();
              }}
              className="h-[20px] px-3 flex items-center text-[11px] text-[var(--text0)] hover:bg-[var(--bg2)] cursor-pointer"
            >
              {item.label}
            </div>
          );
        })}
      </div>
    </>
  );
}
