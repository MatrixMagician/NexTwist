/**
 * Pure presentation formatters. No Svelte, no Tauri — safe to unit-test directly.
 */

/** Human percent for a download row, or null when the total size is unknown. */
export function pct(d: { downloaded: number; total: number | null }): number | null {
  return d.total && d.total > 0 ? Math.floor((d.downloaded / d.total) * 100) : null;
}

/** Human byte size with a single unit step (B / KB / MB / GB). */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
