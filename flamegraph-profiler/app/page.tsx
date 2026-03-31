"use client";

import React, { useState, useCallback } from "react";
import type { ProfileData } from "@/types/profiler";
import { UploadZone } from "@/components/profiler/UploadZone";
import { Dashboard } from "@/components/profiler/Dashboard";
import { DEMO_PROFILE } from "@/lib/demo-data";

type AppState =
  | { screen: "upload" }
  | { screen: "profiler"; data: ProfileData };

export default function Home() {
  const [appState, setAppState] = useState<AppState>({ screen: "upload" });

  const handleLoad = useCallback((data: ProfileData) => {
    setAppState({ screen: "profiler", data });
  }, []);

  const handleLoadDemo = useCallback(() => {
    setAppState({ screen: "profiler", data: DEMO_PROFILE });
  }, []);

  const handleReset = useCallback(() => {
    setAppState({ screen: "upload" });
  }, []);

  if (appState.screen === "profiler") {
    return <Dashboard data={appState.data} onReset={handleReset} />;
  }

  return <UploadZone onLoad={handleLoad} onLoadDemo={handleLoadDemo} />;
}
