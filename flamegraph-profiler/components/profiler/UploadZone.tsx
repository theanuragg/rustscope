"use client";

import React, { useCallback, useRef, useState, useEffect } from "react";
import type { ProfileData } from "@/types/profiler";
import { parseSession } from "@/lib/parse-profile";

interface Props {
  onLoad: (data: ProfileData) => void;
  onLoadDemo: () => void;
}

export function UploadZone({ onLoad, onLoadDemo }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const processFile = useCallback(
    async (file: File) => {
      try {
        const text = await file.text();
        const json = JSON.parse(text);
        const result = parseSession(json);
        if (result.ok && result.data) {
          onLoad(result.data);
        }
      } catch (e) {
        console.error("Failed to parse profile:", e);
      }
    },
    [onLoad]
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) processFile(file);
    },
    [processFile]
  );

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  const onDragLeave = useCallback(() => {
    setIsDragging(false);
  }, []);

  const onFileInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
    },
    [processFile]
  );

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "l") {
        inputRef.current?.click();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <div className="w-full h-full bg-[var(--bg0)] flex flex-col items-center justify-center font-mono">
      <input
        type="file"
        ref={inputRef}
        className="hidden"
        accept=".json"
        onChange={onFileInput}
      />
      
      <div className="text-[14px] text-[var(--text1)] mb-1">
        perf.rs
      </div>
      <div className="text-[11px] text-[var(--text2)] mb-6">
        drop profiling json here or press L to load
      </div>

      <div
        onDrop={onDrop}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onClick={() => inputRef.current?.click()}
        className={`w-[320px] h-[120px] border border-dashed flex items-center justify-center cursor-pointer transition-colors duration-80ms ${
          isDragging ? "border-[var(--accent)]" : "border-[var(--border2)]"
        }`}
      >
        {isDragging ? (
          <span className="text-[var(--accent)] text-[11px]">DROP FILE</span>
        ) : (
          <span className="text-[var(--text3)] text-[11px]">.json file</span>
        )}
      </div>

      <div className="mt-8">
        <button
          onClick={onLoadDemo}
          className="tracy-button"
        >
          LOAD DEMO PROFILE
        </button>
      </div>
    </div>
  );
}
