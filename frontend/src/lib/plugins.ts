/**
 * Pure plugin load-order rules (no Svelte state, no IO).
 *
 * The engine (`crates/loadorder`) is authoritative: it rejects a masters-first violation
 * or a protected-master move regardless of what the UI does. This module is the courtesy
 * layer that keeps the controls honest, and being pure it can be tested directly.
 */
import type { PluginInfo } from "$lib/api";

/** A plugin is in the "masters" group if it is a master (.esm) or ESL-flagged (.esl). */
export const isMaster = (p: PluginInfo): boolean => p.kind === "esm" || p.kind === "esl";

/** The badge text for a plugin's kind. */
export const kindBadge = (k: PluginInfo["kind"]): string => k.toUpperCase();

/**
 * True if swapping `plugins[i]` with its neighbour in `dir` would put a regular plugin
 * before a master (or vice-versa) — a masters-first violation we must prevent (§B.2).
 * Out-of-range moves report `true` because those controls are disabled anyway.
 */
export function violatesMastersFirst(
  plugins: readonly PluginInfo[],
  i: number,
  dir: -1 | 1,
): boolean {
  const other = i + dir;
  if (other < 0 || other >= plugins.length) return true;
  return isMaster(plugins[i]) !== isMaster(plugins[other]);
}

/** True when either endpoint of the swap is an engine-protected master. */
export function touchesProtected(
  plugins: readonly PluginInfo[],
  i: number,
  dir: -1 | 1,
): boolean {
  const other = i + dir;
  if (other < 0 || other >= plugins.length) return false;
  return plugins[i].protected || plugins[other].protected;
}

/**
 * Swap `i` with its neighbour in `dir` and renumber `order`, or return `null` when the
 * move is refused (out of range, protected endpoint, or masters-first violation).
 * Never mutates the input list.
 */
export function reorder(
  plugins: readonly PluginInfo[],
  i: number,
  dir: -1 | 1,
): PluginInfo[] | null {
  const other = i + dir;
  if (other < 0 || other >= plugins.length) return null;
  if (touchesProtected(plugins, i, dir)) return null;
  if (violatesMastersFirst(plugins, i, dir)) return null;
  const next = [...plugins];
  [next[i], next[other]] = [next[other], next[i]];
  return next.map((p, idx) => ({ ...p, order: idx }));
}
