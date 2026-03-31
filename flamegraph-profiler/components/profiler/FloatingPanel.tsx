"use client";

import React, { useState, useRef, useEffect } from "react";

interface Props {
  title: string;
  onClose?: () => void;
  children: React.ReactNode;
  initialX?: number;
  initialY?: number;
  width?: number;
}

export function FloatingPanel({ title, onClose, children, initialX = 100, initialY = 100, width = 280 }: Props) {
  const [pos, setPos] = useState({ x: initialX, y: initialY });
  const [isDragging, setIsDragging] = useState(false);
  const dragStart = useRef({ x: 0, y: 0 });

  const onMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    dragStart.current = { x: e.clientX - pos.x, y: e.clientY - pos.y };
  };

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        setPos({
          x: e.clientX - dragStart.current.x,
          y: e.clientY - dragStart.current.y,
        });
      }
    };
    const onMouseUp = () => setIsDragging(false);

    if (isDragging) {
      window.addEventListener("mousemove", onMouseMove);
      window.addEventListener("mouseup", onMouseUp);
    }
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, [isDragging]);

  return (
    <div
      style={{ left: pos.x, top: pos.y, width }}
      className="absolute bg-[var(--bg1)] border border-[var(--border2)] flex flex-col shadow-none z-50 select-none"
    >
      {/* Title Bar */}
      <div
        onMouseDown={onMouseDown}
        className="h-[20px] bg-[var(--bg0)] border-b border-[var(--border2)] flex items-center justify-between px-2 cursor-move"
      >
        <span className="text-[11px] font-medium text-[var(--text0)]">{title}</span>
        <div className="flex items-center gap-2">
          <button className="text-[10px] text-[var(--text1)] hover:text-[var(--text0)]">P</button>
          {onClose && (
            <button
              onClick={onClose}
              className="text-[10px] text-[var(--text1)] hover:text-[var(--text0)]"
            >
              X
            </button>
          )}
        </div>
      </div>
      {/* Body */}
      <div className="p-2 overflow-auto max-h-[400px]">
        {children}
      </div>
    </div>
  );
}
