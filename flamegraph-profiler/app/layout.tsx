import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "flamegraph · profiler",
  description: "Rust async-aware flame graph profiler. Supports cargo-flamegraph, samply, pprof JSON.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
