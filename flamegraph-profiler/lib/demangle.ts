/**
 * Rust symbol demangler.
 *
 * Handles:
 * - v0 mangling (rustc >= 1.37): _R prefix
 * - legacy mangling: _ZN prefix
 * - Strips hash suffixes like ::h1234abcd
 * - Collapses closure markers {closure#N}
 * - Collapses <T as Trait> patterns for display
 */

const HASH_SUFFIX = /::h[0-9a-f]{8,16}$/;
const CLOSURE    = /\{closure#\d+\}/g;
const SHIM       = /\{shim:.*?\}/g;
const AS_TRAIT   = /<(.+?) as (.+?)>/g;

/** Full demangling — used in tooltips */
export function demangleFull(symbol: string): string {
  let s = symbol.trim();

  // Strip leading underscore on some platforms
  if (s.startsWith("__ZN") || s.startsWith("__R")) s = s.slice(1);

  // Legacy _ZN mangling — very naive strip
  if (s.startsWith("_ZN")) {
    s = s.slice(3).replace(/\d+/g, "::").replace(/::[0-9]+/g, "");
  }

  // v0 _R mangling — we just clean it up, full decode is complex
  if (s.startsWith("_R")) {
    s = s.slice(2);
  }

  // Remove hash suffix
  s = s.replace(HASH_SUFFIX, "");

  // Shims
  s = s.replace(SHIM, "shim");

  return s;
}

/** Short display name — used in frame bars */
export function demangleShort(symbol: string): string {
  const full = demangleFull(symbol);

  // Collapse <T as Trait> → <T>
  let s = full.replace(AS_TRAIT, "<$1>");

  // Collapse closure bodies
  s = s.replace(CLOSURE, "{closure}");

  // Take just the last path segment for very long names
  const parts = s.split("::");
  if (parts.length > 4) {
    // Show crate::…::module::fn
    return `${parts[0]}::…::${parts[parts.length - 2]}::${parts[parts.length - 1]}`;
  }

  return s;
}

/** Extract crate name from symbol */
export function extractCrate(symbol: string): string {
  const full = demangleFull(symbol);
  const first = full.split("::")[0];
  // Strip angle brackets from generic roots
  return first.replace(/[<>]/g, "").trim() || "unknown";
}

/** Format nanoseconds into human-readable string */
export function formatNs(ns: number): string {
  if (ns < 1_000) return `${ns.toFixed(0)} ns`;
  if (ns < 1_000_000) return `${(ns / 1_000).toFixed(2)} µs`;
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  return `${(ns / 1_000_000_000).toFixed(3)} s`;
}

/** Format bytes */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(3)} GiB`;
}
